//! Manual check of Auto's router against hand-labelled requests. Never part of `cargo t`: it talks to the real CLIs with
//! the user's own sign-in and spends a little real quota (cheap models, one-word answers).
//!
//!     cargo run -p faber-engine --example route_eval -- [runs]
//!
//! Each router (the two `ROUTER` models, at medium and at low effort) reads every request in a fresh session, the way
//! Auto asks it, and the table prints how often its verdict matches the label, how often it was valid JSON, and how long
//! it took. A label is one person's reading, so a disagreement is a question to look at, not a proven mistake.

use faber_engine::claude::{ClaudeLaunch, ClaudeOptions, EventSink, start_claude};
use faber_engine::cli::{Cli, find_cli};
use faber_engine::codex::{CodexServer, CodexServerOptions, CodexThread};
use faber_engine::events::{EngineEvent, Session, UserTurn};
use faber_engine::model::{Area, Permissions, Vendor};
use faber_engine::proc::{Spawner, real_spawner};
use faber_engine::route::{INSTRUCTIONS, ROUTER, asked, question};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// (request, kind, difficulty, check), written the way people type to Faber.
const LABELLED: [(&str, &str, &str, &str); 30] = [
    ("oi, tudo bem?", "chat", "easy", "glance"),
    ("What's the capital of Australia?", "chat", "easy", "blind"),
    ("Escreve um email educado pro meu chefe pedindo folga na sexta", "writing", "easy", "glance"),
    ("Translate this paragraph into Spanish: The meeting moved to Thursday because the room was booked.", "writing", "easy", "glance"),
    ("Write a 1,500-word short story about a lighthouse keeper who finds a radio that talks back", "writing", "medium", "glance"),
    ("Monta um plano de estudos de 8 semanas pra eu passar no exame de inglês B2", "writing", "medium", "blind"),
    ("Fix the typo in the README title, it says Fabre instead of Faber", "code", "easy", "glance"),
    ("Add a dark mode toggle to the settings page of my React app", "code", "medium", "glance"),
    ("The test suite fails on CI with a timeout in tests/api.test.ts, find out why and fix it", "code", "medium", "machine"),
    ("Write a Python function that parses ISO 8601 durations like P3DT4H, with unit tests", "code", "medium", "machine"),
    ("Refactor the payment module to use the new billing API across all 14 services without breaking anything", "code", "hard", "machine"),
    ("Review this function for race conditions: it updates a shared cache from several threads without a lock", "code", "hard", "blind"),
    ("Meu script de backup em bash às vezes apaga a pasta errada, dá uma olhada", "code", "hard", "blind"),
    ("Is this SQL query vulnerable to injection? SELECT * FROM users WHERE name = '\" + name + \"'", "code", "medium", "blind"),
    ("Quanto é 17% de 2.340?", "math", "easy", "machine"),
    ("Prove that the square root of 2 is irrational", "math", "medium", "blind"),
    ("Calcula a parcela de um financiamento de 300 mil em 360 meses a 0,9% ao mês", "math", "medium", "machine"),
    ("Find all real solutions of x^4 - 5x^2 + 4 = 0 and check them", "math", "medium", "machine"),
    ("What is the expected number of rolls of a fair die until two sixes appear in a row?", "math", "hard", "blind"),
    ("Compara o iPhone 17 com o Galaxy S26 pra quem tira muita foto", "research", "medium", "blind"),
    ("Explain how vaccines train the immune system, for a 12-year-old", "research", "easy", "glance"),
    ("Should I pay off my car loan early or invest the money in an index fund? I'm 34 and the loan is at 6%", "research", "hard", "blind"),
    ("Quais são os efeitos colaterais de tomar ibuprofeno junto com losartana?", "research", "hard", "blind"),
    ("Summarise the main arguments for and against a four-day work week, with sources", "research", "medium", "blind"),
    ("Posso ser demitido por justa causa se faltar dois dias com atestado?", "research", "hard", "blind"),
    ("Which is better for a small online store in Brazil, Shopify or Nuvemshop?", "research", "medium", "blind"),
    ("Make a logo for my bakery called Pão da Vó, warm colours", "image", "medium", "glance"),
    ("Remove the background from this product photo", "image", "easy", "glance"),
    ("Organize these 200 receipts into a spreadsheet by month and category", "other", "medium", "machine"),
    ("Me lembra de ligar pro dentista amanhã às 10h", "other", "easy", "glance"),
];

fn spawner(cli: &Cli) -> Spawner {
    let (base, prefix, command) = (real_spawner(), cli.prefix.clone(), cli.command.clone());
    Arc::new(move |_bin, args, cwd| base(&command, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd))
}

/// The router's whole reply, or None when it failed or took too long.
async fn reply(rx: &mut UnboundedReceiver<EngineEvent>) -> Option<String> {
    let mut text = String::new();
    loop {
        match tokio::time::timeout(TURN_TIMEOUT, rx.recv()).await.ok()?? {
            EngineEvent::Text(t) => text.push_str(&t),
            EngineEvent::Done { ok, .. } => return ok.then_some(text),
            EngineEvent::Error { fatal: true, .. } => return None,
            _ => {}
        }
    }
}

struct Router {
    dir: PathBuf,
    codex: Option<(Cli, CodexServer)>,
    claude: Option<Cli>,
}

impl Router {
    fn open(&self, vendor: Vendor, model: &str, effort: &str) -> Option<(Session, UnboundedReceiver<EngineEvent>)> {
        let (tx, rx) = unbounded_channel();
        let sink: EventSink = Arc::new(move |e| drop(tx.send(e)));
        let (model, effort, instructions) = (Some(model.to_string()), Some(effort.to_string()), Some(INSTRUCTIONS.to_string()));
        let session = match vendor {
            Vendor::Codex => {
                let (_, server) = self.codex.as_ref()?;
                server.open(
                    CodexThread {
                        cwd: self.dir.clone(),
                        model,
                        effort,
                        resume: None,
                        instructions,
                        user_name: None,
                        permissions: Permissions::Ask,
                    },
                    sink,
                )
            }
            Vendor::Claude => {
                let cli = self.claude.as_ref()?;
                let launch = ClaudeLaunch {
                    area: Area::Chat,
                    model,
                    effort,
                    resume: None,
                    user_hooks: false,
                    instructions,
                    user_name: None,
                    flags: Default::default(),
                    permissions: Permissions::Ask,
                };
                start_claude(
                    ClaudeOptions { bin: cli.command.clone(), cwd: self.dir.clone(), launch, spawn: spawner(cli), sandbox: None },
                    sink,
                )
            }
        };
        Some((session, rx))
    }
}

#[derive(Default)]
struct Score {
    asked: u32,
    valid: u32,
    kind: u32,
    difficulty: u32,
    check: u32,
    secs: Vec<f64>,
    misses: Vec<String>,
}

#[tokio::main]
async fn main() {
    let runs: u32 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(1);
    let dir = std::env::temp_dir().join("faber-route-eval");
    std::fs::create_dir_all(&dir).expect("a folder to work in");
    let codex = find_cli("codex").map(|cli| {
        let server = CodexServer::start(CodexServerOptions {
            permissions: Permissions::Ask,
            cli: cli.clone(),
            cwd: dir.clone(),
            area: Area::Chat,
            user_hooks: false,
            windows: cfg!(windows),
            spawn: spawner(&cli),
        });
        (cli, server)
    });
    let router = Router { dir, codex, claude: find_cli("claude") };
    println!("| router | effort | valid JSON | kind | difficulty | check | median s | slowest s |");
    println!("|---|---|---|---|---|---|---|---|");
    for (vendor, model, _) in ROUTER {
        for effort in ["medium", "low"] {
            let mut score = Score::default();
            for _ in 0..runs {
                for (request, kind, difficulty, check) in LABELLED {
                    let Some((session, mut rx)) = router.open(vendor, model, effort) else { continue };
                    let started = Instant::now();
                    session.send(UserTurn::text(question(request, 0)));
                    let answer = reply(&mut rx).await;
                    session.close();
                    score.asked += 1;
                    score.secs.push(started.elapsed().as_secs_f64());
                    let Some(verdict) = answer.as_deref().and_then(asked) else {
                        score.misses.push(format!("no verdict: {request}"));
                        continue;
                    };
                    score.valid += 1;
                    score.kind += u32::from(verdict.kind == kind);
                    score.difficulty += u32::from(verdict.difficulty == difficulty);
                    score.check += u32::from(verdict.check == check);
                    if verdict.check != check {
                        score.misses.push(format!("check {} (label {check}): {request}", verdict.check));
                    }
                }
            }
            if score.asked == 0 {
                eprintln!("{model} is not available here");
                continue;
            }
            score.secs.sort_by(f64::total_cmp);
            let pct = |n: u32| format!("{:.0}%", f64::from(n) * 100.0 / f64::from(score.asked));
            println!(
                "| {model} | {effort} | {} | {} | {} | {} | {:.1} | {:.1} |",
                pct(score.valid),
                pct(score.kind),
                pct(score.difficulty),
                pct(score.check),
                score.secs[score.secs.len() / 2],
                score.secs.last().copied().unwrap_or_default()
            );
            for miss in &score.misses {
                eprintln!("  {model} {effort}: {miss}");
            }
        }
    }
}
