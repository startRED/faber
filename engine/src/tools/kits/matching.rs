use super::Item;
use std::collections::BTreeSet;
use std::path::Path;

/// Stack tags are authored here alongside the pinned catalog, not read from downloaded skill text.
fn item_stack(id: &str) -> Option<&'static str> {
    match id {
        "swift-concurrency" | "swiftui-expert-skill" => Some("swift"),
        "flutter-add-widget-test" | "flutter-apply-architecture-best-practices" => Some("flutter"),
        "expo-router" => Some("expo"),
        "react-native-best-practices" => Some("react-native"),
        "jetpack-compose-performance" => Some("compose"),
        "prisma-client-api" => Some("prisma"),
        "firebase-firestore" => Some("firestore"),
        "supabase-postgres-best-practices" => Some("postgres"),
        "stripe-best-practices" => Some("stripe"),
        "workers-best-practices" => Some("cloudflare"),
        "auth0" => Some("auth0"),
        id if id.starts_with("android-") => Some("android"),
        id if id.starts_with("godot-") => Some("godot"),
        id if id.starts_with("unity-") => Some("unity"),
        id if id.starts_with("roblox-") => Some("roblox"),
        id if id.starts_with("minecraft-") => Some("minecraft"),
        "ue-cpp-foundations" | "unreal-blueprints" => Some("unreal"),
        "bevy-ecs" => Some("bevy"),
        "love2d-game-development" => Some("love2d"),
        "phaser-core" | "phaser-arcade-physics" => Some("phaser"),
        "pixijs-rendering" => Some("pixijs"),
        "threejs-materials-lighting" => Some("threejs"),
        _ => None,
    }
}

pub(super) fn item_capability(id: &str) -> &'static str {
    match id {
        "systematic-debugging" => "Diagnose a software failure before changing code",
        "test-driven-development" => "Implement a software change through regression tests",
        "ponytail" => "Keep implementation small and reuse existing code",
        "design-taste-frontend" => "Design frontend layout and visual direction",
        "improve-codebase-architecture" => "Improve software module boundaries",
        "grill-me" => "Clarify a proposal through focused questions",
        "mcp-builder" => "Build an MCP tool server",
        "skill-creator" => "Create or improve an agent instruction skill",
        "emil-design-eng" => "Polish interface interactions and visual details",
        "animation-performance" => "Diagnose and improve interface animation performance",
        "supabase-postgres-best-practices" => "Optimize Postgres database queries and schema",
        "stripe-best-practices" => "Implement Stripe billing and payment integrations",
        "workers-best-practices" => "Implement Cloudflare Workers services",
        "prisma-client-api" => "Use Prisma database client APIs",
        "firebase-firestore" => "Implement Firebase Firestore storage",
        "react-native-best-practices" => "Improve React Native applications",
        "auth0" => "Implement Auth0 authentication",
        "expo-router" => "Implement Expo Router navigation",
        "swift-concurrency" => "Diagnose and implement Swift concurrency",
        "flutter-add-widget-test" => "Add Flutter widget tests",
        "jetpack-compose-performance" => "Improve Jetpack Compose rendering performance",
        "chrome-devtools" => "Measure and diagnose browser rendering and performance",
        _ => "Catalog instructions for this capability",
    }
}

pub(super) fn matches_task(item: &Item, request: &str, stacks: &BTreeSet<&str>) -> bool {
    if !matches_stack(item, stacks) {
        return false;
    }
    let text = request_text(request);
    let has = |terms: &str| terms.split('|').any(|term| contains_term(&text, term));
    // A shared job is a shortlist, not permission to load unrelated specializations.
    let intent = match item.id.as_str() {
        "animate" => {
            Some("animation|animate|animacao|animar|animacion|animer|animationen|animieren|transition|transicao|transicion|ubergang")
        }
        "gsap-core" => Some("gsap"),
        "break-ui" => Some("interface|ui|frontend|front-end|website|site|sitio|webseite|oberflache|tela|pantalla|ecran"),
        "grill-me" => Some(
            "proposal|proposta|propuesta|proposition|vorschlag|planning|planejar|planejamento|planificar|planificacion|planifier|planen|planung|question|perguntas|preguntas|questions|fragen",
        ),
        "create-prd" => Some(
            "prd|requirements|requisitos|exigences|anforderungen|product specification|especificacao do produto|especificacion del producto|cahier des charges|produktspezifikation",
        ),
        "market-sizing" => Some(
            "market size|market sizing|tamanho do mercado|dimensionar o mercado|tamano del mercado|taille du marche|marktgrosse|marktvolumen|tam|sam|som",
        ),
        "pm-pre-mortem" => Some("pre-mortem|premortem|risks|riscos|riesgos|risques|risiken"),
        "pm-release-notes" => Some("release notes|notas de versao|notas da versao|notas de lanzamiento|notes de version|versionshinweise"),
        "pm-summarize-meeting" => Some("meeting|reuniao|reunion|besprechung|sitzung"),
        "pm-user-personas" => Some("persona|personas|target audience|publico-alvo|publico objetivo|public cible|zielgruppe"),
        "pm-interview-script" => Some("interview script|roteiro de entrevista|guion de entrevista|guide d'entretien|interviewleitfaden"),
        "pm-summarize-interview" => Some(
            "summarize interview|resuma a entrevista|resumir entrevista|resume la entrevista|resumer l'entretien|interview zusammenfassen",
        ),
        "pm-prioritize-features" => Some("prioritize|priorizar|priorise|prioriser|priorisieren|roadmap"),
        "pm-business-model" => Some("business model|modelo de negocio|modele economique|geschaftsmodell"),
        "pm-product-strategy" => Some("product strategy|estrategia de produto|estrategia de producto|strategie produit|produktstrategie"),
        "pm-ab-test-analysis" | "marketing-ab-testing" => Some("a/b|ab test|split test|teste ab|test ab"),
        "pm-cohort-analysis" => Some("cohort|coorte|cohorte|kohorte|retention|retencao|retencion|fidelisation|bindung"),
        "pm-metrics-dashboard" => Some("dashboard|metrics|metricas|indicateurs|kennzahlen|tableau de bord"),
        "pm-sql-queries" => Some("sql|query|queries|schema|esquema|consulta|consultas|requete|requetes|abfrage|abfragen|datenbankschema"),
        "humanizer" => Some(
            "humanize|humanise|humanizar|humanize|humaniza|humaniser|humanisez|menschlicher|natural|naturel|naturlich|robotic|robotico|robotique|roboterhaft|ki-text",
        ),
        "json-canvas" => Some("canvas|mind map|mapa mental|carte mentale|mindmap"),
        "obsidian-bases" => Some("obsidian bases|obsidian database|base do obsidian|bases de obsidian|base obsidian|obsidian-datenbank"),
        "obsidian-markdown" => Some("obsidian|markdown"),
        "copywriting" => Some(
            "copy|sales copy|advert|anuncio|anuncios|publicidade|publicitario|publicitaire|werbetext|werbung|persuasive|persuasivo|persuasif|uberzeugend",
        ),
        "copy-editing" => Some("edit|revise|review|revisar|revise|corrigir|corrige|corregir|reviser|corriger|korrigiere|uberarbeite"),
        "pricing" => Some("pricing|price|prices|preco|precos|precio|precios|tarification|prix|preis|preise|preisgestaltung"),
        "marketing-aso" => Some("app store|aso|play store"),
        "marketing-community-marketing" => Some("community|comunidade|comunidad|communaute|gemeinschaft"),
        "marketing-competitors" => {
            Some("competitor|competitors|concorrente|concorrentes|competencia|competidores|concurrence|concurrents|wettbewerber|konkurrenz")
        }
        "marketing-events" => Some("event|events|evento|eventos|evenement|evenements|veranstaltung|veranstaltungen"),
        "marketing-free-tools" => {
            Some("free tool|ferramenta gratis|ferramenta gratuita|herramienta gratuita|outil gratuit|kostenloses tool")
        }
        "marketing-lead-magnets" => Some("lead magnet|lead magnets|isca digital|iman de leads|aimant a prospects|leadmagnet"),
        "marketing-seo-audit" => Some("seo|search ranking|buscadores|mecanismos de busca|referencement|suchmaschinen"),
        "minecraft-server-admin" => Some("server|servidor|serveur"),
        "minecraft-modding" => Some("mod|mods|modding|neoforge|fabric|forge"),
        "swiftui-expert-skill" => Some("swiftui|interface|ui|view|tela|pantalla|ecran|ansicht"),
        "swift-concurrency" => Some("swift|concurrency|concorrencia|concurrencia|concurrence|nebenlaufigkeit|async|actor"),
        "android-intent-security" => Some("intent|intents"),
        "android-permissions-security" => Some("permission|permissions|permissao|permissoes|permisos|autorisations|berechtigungen"),
        "flutter-apply-architecture-best-practices" => Some("architecture|arquitetura|arquitectura|architektur"),
        "ponytail-debt" => Some("debt|divida|deuda|dette|schulden"),
        _ => None,
    };
    if let Some(terms) = intent {
        return has(terms);
    }
    if item.job == "game-design" {
        return game_intent(&item.id).is_some_and(has);
    }
    if item.job == "games" {
        return game_intent(&item.id).is_none_or(has);
    }
    if item.id == "animation-performance" {
        let text = request.to_lowercase();
        if ![
            "animat",
            "animaç",
            "animac",
            "animação",
            "animation",
            "transition",
            "transiç",
            "transic",
            "übergang",
            "uebergang",
            "interface",
            "frontend",
            "front-end",
            "css",
            "scroll",
            "défilement",
            "rolagem",
            "desplazamiento",
        ]
        .iter()
        .any(|word| text.contains(word))
        {
            return false;
        }
    }
    true
}

fn contains_term(text: &str, term: &str) -> bool {
    text.match_indices(term).any(|(at, _)| {
        let end = at + term.len();
        (at == 0 || !text[..at].chars().next_back().is_some_and(char::is_alphanumeric))
            && (end == text.len() || !text[end..].chars().next().is_some_and(char::is_alphanumeric))
    })
}

fn game_intent(id: &str) -> Option<&'static str> {
    match id {
        "game-audio-design" | "godot-audio" => Some("audio|sound|som|sonido|son|klang"),
        "game-camera-systems" => Some("camera|cameras|kamera"),
        "game-dialogue-systems" => Some("dialogue|dialog|dialogo|dialogos|dialogue|dialogues"),
        "game-ai-systems" => Some("ai|ia|npc|npcs|enemy|enemies|inimigo|enemigo|ennemi|gegner"),
        "game-feel" => Some("game feel|feedback|juice|hitstop|sensacao|ressenti|spielgefuhl"),
        "game-ui-ux" | "godot-ui-control" | "roblox-ui" => Some("ui|ux|hud|interface|menu|menus|oberflache"),
        "game-input-systems" | "unity-input-system" => Some("input|controls|controles|commandes|steuerung|eingabe|controller|gamepad"),
        "game-level-design" => Some("level|levels|fase|fases|nivel|niveles|niveau|niveaux|leveldesign"),
        "game-performance-optimization" => {
            Some("performance|fps|desempenho|rendimiento|leistung|optimisation|otimizacao|optimizacion|optimierung")
        }
        "game-physics-tuning" | "godot-physics" | "roblox-physics" | "phaser-arcade-physics" => {
            Some("physics|fisica|physique|physik|collision|colisao|colision|kollision")
        }
        "game-procedural-generation" => Some("procedural|procedurale|prozedural|procedurally"),
        "game-save-systems" => Some("save|saves|saving|salvar|salvamento|guardar|partida|sauvegarde|speichern|spielstand"),
        "game-shader-programming" | "godot-shaders" => Some("shader|shaders|glsl|hlsl"),
        "game-card-design" => Some("card|cards|cartas|cartes|kartenspiel"),
        "game-fps-shooter" => Some("shooter|fps|tiro|disparos|tir"),
        "game-platformer-design" => Some("platformer|plataforma|plataformas|plateforme|jump-and-run"),
        "game-puzzle-design" => Some("puzzle|puzzles|quebra-cabeca|rompecabezas|enigme|ratsel"),
        "game-roguelike-design" => Some("roguelike|roguelite"),
        "game-rpg-design" => Some("rpg|role-playing"),
        "game-survival-crafting" => Some("survival|sobrevivencia|supervivencia|survie|uberleben|crafting"),
        "game-tower-defense-design" => Some("tower defense|defesa de torres|defensa de torres|defense de tours|turmverteidigung"),
        "game-visual-novel-design" => Some("visual novel|novela visual|roman visuel"),
        "game-prototype-fast" => Some("prototype|prototipo|prototyp"),
        "godot-2d-movement" => Some("movement|movimento|movimiento|deplacement|bewegung"),
        "godot-3d-essentials" => Some("3d"),
        "godot-animation" | "unity-animation" => Some("animation|animacao|animacion|animieren"),
        "godot-gdscript-headless-testing" => Some("test|tests|testes|pruebas|headless"),
        "godot-multiplayer" | "roblox-networking" => Some("multiplayer|network|rede|red|reseau|netzwerk"),
        "godot-tilemap" => Some("tilemap|tiles|tilemaps"),
        "roblox-characters" => Some("character|characters|personagem|personaje|personnage|figur|avatar"),
        "roblox-datastores" => Some("datastore|datastores|save|salvar|guardar|sauvegarder|speichern"),
        "unity-build-pipeline" => Some("build|pipeline|compilar|compilacao|compilacion|compiler|kompilieren"),
        "unity-scriptableobjects" => Some("scriptableobject|scriptableobjects"),
        "unreal-blueprints" => Some("blueprint|blueprints"),
        "pixijs-rendering" => Some("render|rendering|renderizacao|renderizado|rendu|darstellung"),
        "threejs-materials-lighting" => Some("material|materials|lighting|luz|iluminacao|iluminacion|eclairage|beleuchtung"),
        _ => None,
    }
}

fn matches_stack(item: &Item, stacks: &BTreeSet<&str>) -> bool {
    item_stack(&item.id).is_none_or(|stack| stacks.contains(stack))
}

fn request_text(request: &str) -> String {
    request
        .chars()
        .take(16_384)
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'î' | 'ï' => 'i',
            'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            _ => c,
        })
        .collect::<String>()
        .replace('ß', "ss")
}

pub(super) fn stack_labels(request: &str, cwd: Option<&Path>) -> BTreeSet<&'static str> {
    let text = request_text(request);
    let tokens: BTreeSet<_> = text.split(|c: char| !c.is_alphanumeric()).collect();
    let mut stacks = BTreeSet::new();
    for (stack, words) in [
        ("swift", "swift swiftui"),
        ("flutter", "flutter dart"),
        ("expo", "expo"),
        ("react-native", "react-native"),
        ("compose", "compose jetpack"),
        ("prisma", "prisma"),
        ("firestore", "firestore firebase"),
        ("postgres", "postgres postgresql supabase sql"),
        ("stripe", "stripe"),
        ("cloudflare", "cloudflare wrangler"),
        ("auth0", "auth0"),
        ("android", "android jetpack"),
        ("godot", "godot gdscript"),
        ("unity", "unity"),
        ("roblox", "roblox luau"),
        ("minecraft", "minecraft neoforge fabric"),
        ("unreal", "unreal ue5"),
        ("bevy", "bevy"),
        ("love2d", "love2d love2d-core"),
        ("phaser", "phaser"),
        ("pixijs", "pixijs"),
        ("threejs", "threejs three.js"),
    ] {
        if words.split_whitespace().any(|word| tokens.contains(word)) {
            stacks.insert(stack);
        }
    }
    if text.contains("react native") || text.contains("react-native") {
        stacks.insert("react-native");
    }
    if text.contains("three.js") {
        stacks.insert("threejs");
    }
    if let Some(cwd) = cwd {
        for (file, stack) in [
            ("Package.swift", "swift"),
            ("pubspec.yaml", "flutter"),
            ("schema.prisma", "prisma"),
            ("prisma/schema.prisma", "prisma"),
            ("wrangler.toml", "cloudflare"),
            ("wrangler.jsonc", "cloudflare"),
        ] {
            if std::fs::symlink_metadata(cwd.join(file)).is_ok_and(|m| m.is_file()) {
                stacks.insert(stack);
            }
        }
    }
    stacks
}

pub(super) fn labels(request: &str, cwd: Option<&Path>) -> (BTreeSet<String>, BTreeSet<String>) {
    let text = request_text(request);
    let tokens: BTreeSet<_> = text.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()).collect();
    let mut jobs = BTreeSet::new();
    for (job, words) in [
        (
            "debugging",
            "debug debugging bug bugs broken crash crashes falha depurar travando trava travou falla fallos bloquea bloqueado depura depurar plantage deboguer absturz absturze absturzt sturzt",
        ),
        ("testing", "test tests testing coverage teste testes testar pruebas probar tester couverture testen testabdeckung"),
        ("design", "design layout frontend css ui ux animate animar animer animieren gsap"),
        (
            "architecture",
            "architecture architectural refactor restructure arquitetura refatorar arquitectura refactorizar architektur refaktorieren restructurer",
        ),
        (
            "plan",
            "planning brainstorm strategy proposal planejar planejamento estrategia proposta planificar planificacion propuesta planifier strategie proposition planen planung vorschlag prd requirements requisitos exigences anforderungen roadmap persona personas premortem",
        ),
        (
            "performance",
            "performance performances optimize optimization fps memory cpu gpu desempenho otimizar otimize memoria optimizar optimiza rendimiento optimiser optimise memoire optimieren optimiere leistung speicher",
        ),
        (
            "database",
            "database databases sql postgres postgresql prisma supabase firestore sqlite firebase datenbank datenbankschema requete requetes consultas",
        ),
        ("payments", "stripe billing checkout faturamento facturacion facturation abrechnung"),
        ("platform", "cloudflare wrangler deploy deployment implantar despliegue deploiement bereitstellen"),
        ("mobile", "swift swiftui flutter dart expo react-native compose jetpack android"),
        ("auth", "auth authentication oauth auth0 autenticacao autenticar autenticacion authentification authentifizierung"),
        ("mcp", "mcp"),
        ("measure", "lighthouse devtools vitals benchmark benchmarks"),
        (
            "lean",
            "code coding build implement implementation programming codigo programar implementar implementacao implementacion programmer implementer programmieren implementieren",
        ),
        ("writing", "write writing article essay texto escrever artigo redacao"),
        ("research", "research investigate sources pesquisar pesquisa fontes"),
        ("image", "image images photo illustration imagem imagens foto ilustracao"),
        ("video", "video animation animacao film filme"),
        ("spreadsheet", "spreadsheet excel csv planilha planilhas"),
        ("presentation", "presentation slides powerpoint apresentacao apresentacoes"),
    ] {
        if words.split_whitespace().any(|word| tokens.contains(word)) {
            jobs.insert(job.into());
        }
    }
    let has = |words: &str| words.split_whitespace().any(|word| tokens.contains(word));
    if jobs.contains("performance") && has("browser browsers navegador navegadores navigateur navigateurs") {
        jobs.insert("measure".into());
    }
    let phrase = |terms: &str| terms.split('|').any(|term| contains_term(&text, term));
    for (job, terms) in [
        (
            "documents",
            "humanize|humanise|humanizar|humaniza|humaniser|humanisez|menschlicher|ki-text|obsidian|markdown|mind map|mapa mental|carte mentale|mindmap",
        ),
        (
            "marketing",
            "marketing|copywriting|sales copy|advertising|anuncio|anuncios|publicidade|publicitario|publicitaire|werbetext|werbung|pricing|precificacao|precio|tarification|preisgestaltung|seo|aso|lead magnet",
        ),
        ("games", "minecraft|godot|gdscript|unity|roblox|luau|unreal|bevy|love2d|phaser|pixijs|threejs|three.js"),
        (
            "plan",
            "product strategy|estrategia de produto|estrategia de producto|strategie produit|produktstrategie|business model|modelo de negocio|modele economique|geschaftsmodell|market size|market sizing|tamanho do mercado|dimensionar o mercado|tamano del mercado|taille du marche|marktgrosse|marktvolumen|release notes|notas de versao|notas da versao|notas de lanzamiento|notes de version|versionshinweise|meeting|reuniao|reunion|besprechung|sitzung|prioritize|priorizar|prioriser|priorisieren|interview|entrevista|entretien",
        ),
        (
            "database",
            "banco de dados|base de dados|base de datos|base de donnees|a/b|ab test|cohort|coorte|cohorte|kohorte|metrics dashboard|painel de metricas|tableau de bord|kennzahlen",
        ),
    ] {
        if phrase(terms) {
            jobs.insert(job.into());
        }
    }
    if has("game games jogo jogos juego juegos jeu jeux spiel spiele videogame videojuego") {
        jobs.insert("game-design".into());
    }
    let artifact = has(
        "app apps application applications aplicativo aplicacao aplicaciones aplicacion logiciel software programm anwendung website webpage site sitio web webseite verkaufsseite landing interface interfase oberflache",
    ) || (has("page pagina seite") && has("sales vendas ventas vente verkauf"));
    let creation = has(
        "create build make implement criar crie cria faca fazer construir desenvolver crea crear construye hacer haz desarrolla cree creer creez fais faire construis construire developpe erstelle erstellen baue bauen entwickle entwickeln",
    );
    let improvement = has(
        "improve redesign polish melhore melhorar redesenhe mejora mejorar rediseña ameliorer ameliore refais verbessern verbessere gestalte",
    );
    if artifact && (creation || improvement) {
        jobs.insert("design".into());
    }
    if jobs.contains("database") && !artifact && !has("ui ux interface frontend css") {
        jobs.remove("design");
    }
    if artifact
        && has(
            "fix repair consertar conserta corrigir corrige arregla arreglar corrige reparer repare corriger corrigez repariere reparieren behebe beheben",
        )
    {
        jobs.insert("debugging".into());
    }
    if artifact && has("error errors erro erros erreur erreurs fehler plante") {
        jobs.insert("debugging".into());
    }
    if [
        "nao funciona",
        "doesn't work",
        "does not work",
        "no funciona",
        "ne fonctionne pas",
        "funktioniert nicht",
        "sturz beim",
        "sturzt ab",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
    {
        jobs.insert("debugging".into());
    }
    if has("typo digitacao errata frappe tippfehler") && !has("crash crashes trava travando bloquea plante absturz") {
        jobs.remove("debugging");
    }
    if text.contains("react native") || text.contains("react-native") {
        jobs.insert("mobile".into());
    }
    if (creation || improvement) && has("skill skills") {
        jobs.insert("skills".into());
    }
    let explicit = jobs.clone();
    if let Some(cwd) = cwd {
        // Fixed filenames only: no recursion, file content, configuration or credentials leave the PC.
        for (file, job) in [
            ("Cargo.toml", "lean"),
            ("package.json", "lean"),
            ("pyproject.toml", "lean"),
            ("go.mod", "lean"),
            ("pubspec.yaml", "mobile"),
            ("Package.swift", "mobile"),
            ("build.gradle", "mobile"),
            ("wrangler.toml", "platform"),
            ("wrangler.jsonc", "platform"),
            ("schema.prisma", "database"),
            ("prisma/schema.prisma", "database"),
        ] {
            if std::fs::symlink_metadata(cwd.join(file)).is_ok_and(|m| m.is_file()) {
                jobs.insert(job.into());
            }
        }
    }
    (jobs, explicit)
}

#[cfg(test)]
mod tests {
    use super::*;

    // These are ordinary requests, not catalog IDs or translations supplied to a model.
    // Each row covers one real shipped job, with an adjacent-topic counterexample per locale.
    #[test]
    fn every_shipped_job_matches_real_requests_in_all_five_languages() {
        let cases = [
            (
                "systematic-debugging",
                [
                    "My app crashes when I open the settings",
                    "Meu aplicativo trava ao abrir as configurações",
                    "Mi aplicación se bloquea al abrir los ajustes",
                    "Mon application plante au démarrage",
                    "Meine App stürzt beim Öffnen ab",
                ],
                [
                    "Fix the typo in README",
                    "Corrija o erro de digitação no README",
                    "Corrige la errata en README",
                    "Corrige la faute de frappe dans README",
                    "Korrigiere den Tippfehler in README",
                ],
            ),
            (
                "test-driven-development",
                [
                    "Add regression tests for the checkout function",
                    "Adicione testes de regressão para a função de checkout",
                    "Añade pruebas de regresión para la función de pago",
                    "Ajoute des tests de régression pour la fonction de paiement",
                    "Schreibe Tests für die Zahlungsfunktion",
                ],
                [
                    "Help me prepare for tomorrow's exam",
                    "Me ajude a estudar pra prova de amanhã",
                    "Ayúdame a estudiar para el examen de mañana",
                    "Aide-moi à préparer mon examen de demain",
                    "Hilf mir bei der Prüfungsvorbereitung",
                ],
            ),
            (
                "ponytail",
                [
                    "Implement a small code change to save the form",
                    "Implemente uma mudança pequena no código para salvar o formulário",
                    "Implementa un cambio pequeño en el código para guardar el formulario",
                    "Implémente un petit changement dans le code pour enregistrer le formulaire",
                    "Implementiere eine kleine Änderung im Code zum Speichern des Formulars",
                ],
                [
                    "Suggest a quick dinner recipe",
                    "Sugira uma receita rápida pro jantar",
                    "Sugiere una receta rápida para cenar",
                    "Propose une recette rapide pour dîner",
                    "Empfiehl ein schnelles Abendessen",
                ],
            ),
            (
                "design-taste-frontend",
                [
                    "Create a website for my bakery",
                    "Crie um site pra minha padaria",
                    "Crea un sitio para mi panadería",
                    "Crée un site pour ma boulangerie",
                    "Erstelle eine Website für meine Bäckerei",
                ],
                [
                    "Design the database schema",
                    "Desenhe o esquema do banco de dados",
                    "Diseña el esquema de la base de datos",
                    "Conçois le schéma de la base de données",
                    "Entwirf das Datenbankschema",
                ],
            ),
            (
                "improve-codebase-architecture",
                [
                    "Refactor the software architecture into smaller modules",
                    "Melhore a arquitetura do código separando os módulos",
                    "Mejora la arquitectura del código separando los módulos",
                    "Améliore l'architecture du code en séparant les modules",
                    "Verbessere die Architektur des Codes mit kleineren Modulen",
                ],
                [
                    "Draw a floor plan for my house",
                    "Desenhe a planta da minha casa",
                    "Dibuja un plano de mi casa",
                    "Dessine le plan de ma maison",
                    "Zeichne einen Grundriss meines Hauses",
                ],
            ),
            (
                "grill-me",
                [
                    "Challenge my proposal before planning the new product",
                    "Questione minha proposta antes de planejar o novo produto",
                    "Cuestiona mi propuesta antes de planificar el nuevo producto",
                    "Pose des questions sur ma proposition avant de planifier le produit",
                    "Hinterfrage meinen Vorschlag vor der Planung des neuen Produkts",
                ],
                [
                    "Summarize the meeting decisions",
                    "Resuma as decisões da reunião",
                    "Resume las decisiones de la reunión",
                    "Résume les décisions de la réunion",
                    "Fasse die Entscheidungen der Besprechung zusammen",
                ],
            ),
            (
                "mcp-builder",
                [
                    "Build an MCP server for our weather API",
                    "Crie um servidor MCP para nossa API de clima",
                    "Crea un servidor MCP para nuestra API del tiempo",
                    "Crée un serveur MCP pour notre API météo",
                    "Erstelle einen MCP-Server für unsere Wetter-API",
                ],
                [
                    "Explain tomorrow's weather forecast",
                    "Explique a previsão do tempo de amanhã",
                    "Explica el pronóstico del tiempo de mañana",
                    "Explique les prévisions météo de demain",
                    "Erkläre die Wettervorhersage für morgen",
                ],
            ),
            (
                "skill-creator",
                [
                    "Create a skill to review accessibility",
                    "Crie uma skill para revisar acessibilidade",
                    "Crea una skill para revisar accesibilidad",
                    "Crée une skill pour vérifier l'accessibilité",
                    "Erstelle eine Skill zur Prüfung der Barrierefreiheit",
                ],
                [
                    "What are your skills?",
                    "Quais são suas habilidades?",
                    "¿Cuáles son tus habilidades?",
                    "Quelles sont tes compétences ?",
                    "Welche Fähigkeiten hast du?",
                ],
            ),
            (
                "animation-performance",
                [
                    "Optimize the stuttering CSS animation",
                    "Otimize a animação CSS que está travando",
                    "Optimiza la animación CSS que va a tirones",
                    "Optimise cette animation CSS saccadée",
                    "Optimiere die ruckelnde CSS-Animation",
                ],
                [
                    "Optimize the Postgres query",
                    "Otimize a consulta do Postgres",
                    "Optimiza la consulta de Postgres",
                    "Optimise la requête Postgres",
                    "Optimiere die Postgres-Abfrage",
                ],
            ),
            (
                "pm-sql-queries",
                [
                    "Write a SQL query for monthly revenue",
                    "Escreva uma consulta SQL para o faturamento mensal",
                    "Escribe una consulta SQL para los ingresos mensuales",
                    "Écris une requête SQL pour le chiffre d'affaires mensuel",
                    "Schreibe eine SQL-Abfrage für den Monatsumsatz",
                ],
                [
                    "Analyze retention by cohort",
                    "Analise a retenção por coorte",
                    "Analiza la retención por cohorte",
                    "Analyse la fidélisation par cohorte",
                    "Analysiere die Kundenbindung je Kohorte",
                ],
            ),
            (
                "stripe-best-practices",
                [
                    "Integrate Stripe subscriptions in our app",
                    "Integre assinaturas Stripe no nosso aplicativo",
                    "Integra suscripciones Stripe en nuestra aplicación",
                    "Intègre les abonnements Stripe dans notre application",
                    "Integriere Stripe-Abonnements in unsere Anwendung",
                ],
                [
                    "Compare my bank's account fees",
                    "Compare as tarifas da minha conta bancária",
                    "Compara las comisiones de mi cuenta bancaria",
                    "Compare les frais de mon compte bancaire",
                    "Vergleiche die Gebühren meines Bankkontos",
                ],
            ),
            (
                "workers-best-practices",
                [
                    "Deploy an API using Cloudflare Workers",
                    "Publique uma API usando Cloudflare Workers",
                    "Publica una API con Cloudflare Workers",
                    "Déploie une API avec Cloudflare Workers",
                    "Stelle eine API mit Cloudflare Workers bereit",
                ],
                [
                    "Explain how clouds form",
                    "Explique como as nuvens se formam",
                    "Explica cómo se forman las nubes",
                    "Explique comment se forment les nuages",
                    "Erkläre die Entstehung von Wolken",
                ],
            ),
            (
                "swift-concurrency",
                [
                    "Fix Swift concurrency in my iOS app",
                    "Corrija a concorrência Swift no meu aplicativo iOS",
                    "Corrige la concurrencia Swift en mi aplicación iOS",
                    "Corrige la concurrence Swift de mon application iOS",
                    "Behebe die Swift-Nebenläufigkeit in meiner iOS-App",
                ],
                [
                    "Add Flutter widget tests",
                    "Adicione testes de widgets Flutter",
                    "Añade pruebas de widgets Flutter",
                    "Ajoute des tests de widgets Flutter",
                    "Schreibe Flutter-Widget-Tests",
                ],
            ),
            (
                "auth0",
                [
                    "Set up Auth0 authentication for our app",
                    "Configure a autenticação Auth0 do nosso aplicativo",
                    "Configura la autenticación Auth0 de nuestra aplicación",
                    "Configure l'authentification Auth0 de notre application",
                    "Richte die Auth0-Authentifizierung unserer Anwendung ein",
                ],
                [
                    "How do I recover my personal email password?",
                    "Como recupero a senha do meu e-mail pessoal?",
                    "¿Cómo recupero la contraseña de mi correo personal?",
                    "Comment récupérer le mot de passe de ma messagerie personnelle ?",
                    "Wie stelle ich mein privates E-Mail-Passwort wieder her?",
                ],
            ),
            (
                "humanizer",
                [
                    "Humanize this text so it sounds less robotic",
                    "Humanize este texto pra ele soar menos robótico",
                    "Humaniza este texto para que suene menos robótico",
                    "Humanise ce texte pour qu'il paraisse moins robotique",
                    "Mache diesen KI-Text menschlicher",
                ],
                [
                    "Organize my Obsidian markdown notes",
                    "Organize minhas notas Markdown do Obsidian",
                    "Organiza mis notas Markdown de Obsidian",
                    "Organise mes notes Markdown dans Obsidian",
                    "Ordne meine Markdown-Notizen in Obsidian",
                ],
            ),
            (
                "copywriting",
                [
                    "Write persuasive advertising copy for our launch",
                    "Escreva um texto persuasivo para o anúncio do lançamento",
                    "Escribe un texto persuasivo para el anuncio del lanzamiento",
                    "Écris un texte publicitaire persuasif pour le lancement",
                    "Schreibe einen überzeugenden Werbetext für den Produktstart",
                ],
                [
                    "Audit our website SEO",
                    "Faça uma auditoria de SEO do nosso site",
                    "Haz una auditoría SEO de nuestro sitio",
                    "Fais un audit SEO de notre site",
                    "Prüfe die SEO unserer Website",
                ],
            ),
            (
                "minecraft-server-admin",
                [
                    "Configure my Minecraft server for ten friends",
                    "Configure meu servidor Minecraft para dez amigos",
                    "Configura mi servidor Minecraft para diez amigos",
                    "Configure mon serveur Minecraft pour dix amis",
                    "Richte meinen Minecraft-Server für zehn Freunde ein",
                ],
                [
                    "Create a Minecraft mod with Fabric",
                    "Crie um mod Minecraft com Fabric",
                    "Crea un mod de Minecraft con Fabric",
                    "Crée un mod Minecraft avec Fabric",
                    "Erstelle einen Minecraft-Mod mit Fabric",
                ],
            ),
            (
                "game-level-design",
                [
                    "Design the first level of my puzzle game",
                    "Desenhe a primeira fase do meu jogo de quebra-cabeça",
                    "Diseña el primer nivel de mi juego de rompecabezas",
                    "Conçois le premier niveau de mon jeu de puzzle",
                    "Entwirf das erste Level für mein Spiel",
                ],
                [
                    "Design the save system for my game",
                    "Desenhe o sistema de salvamento do meu jogo",
                    "Diseña el sistema para guardar la partida de mi juego",
                    "Conçois le système de sauvegarde de mon jeu",
                    "Entwirf das Spielstand-System für mein Spiel",
                ],
            ),
            (
                "chrome-devtools",
                [
                    "Measure my page performance with Lighthouse",
                    "Meça o desempenho da minha página com Lighthouse",
                    "Mide el rendimiento de mi página con Lighthouse",
                    "Mesure les performances de ma page avec Lighthouse",
                    "Miss die Leistung meiner Webseite mit Lighthouse",
                ],
                [
                    "Create a website for my bakery",
                    "Crie um site pra minha padaria",
                    "Crea un sitio para mi panadería",
                    "Crée un site pour ma boulangerie",
                    "Erstelle eine Website für meine Bäckerei",
                ],
            ),
            (
                "chrome-devtools",
                [
                    "Measure browser rendering performance",
                    "Meça o desempenho da página no navegador",
                    "Mide el rendimiento de la página en el navegador",
                    "Mesure les performances de la page dans le navigateur",
                    "Miss die Leistung meiner Webseite im Browser",
                ],
                [
                    "Measure backend performance",
                    "Meça o desempenho do servidor",
                    "Mide el rendimiento del servidor",
                    "Mesure les performances de mon serveur",
                    "Miss die Leistung meines Servers",
                ],
            ),
        ];
        let items = crate::tools::catalog();
        let temp = tempfile::tempdir().unwrap();
        let store = crate::tools::Store::with_catalog(temp.path(), items.to_vec());
        let mut installed = std::collections::BTreeMap::new();
        for item in items {
            let marker = store.dir(item).join(item.server.as_ref().map_or("SKILL.md", |server| server.entry.as_str()));
            std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
            std::fs::write(marker, "Matching fixture; not delivered as instructions").unwrap();
            installed.insert(item.id.clone(), item.commit.clone());
        }
        std::fs::write(store.ledger(), serde_json::to_vec(&installed).unwrap()).unwrap();
        let mut covered = BTreeSet::new();
        for (id, positives, negatives) in cases {
            let item = items.iter().find(|item| item.id == id).unwrap();
            covered.insert(item.job.as_str());
            for ((locale, positive), negative) in ["en", "pt", "es", "fr", "de"].into_iter().zip(positives).zip(negatives) {
                let selected = |request| {
                    let jobs = labels(request, None).0;
                    jobs.contains(&item.job) && matches_task(item, request, &stack_labels(request, None))
                };
                assert!(selected(positive), "{locale}, {}: {positive}", item.id);
                assert!(!selected(negative), "{locale}, {}: {negative}", item.id);
                for (request, expected) in [(positive, true), (negative, false)] {
                    let choice = store.shortlist(request, None, crate::model::Area::Project, crate::model::Permissions::FullAccess, &[]);
                    assert_eq!(choice.ids.contains(&item.id), expected, "installed {locale}, {}: {request}: {:?}", item.id, choice.ids);
                }
            }
        }
        assert_eq!(covered, items.iter().map(|item| item.job.as_str()).collect(), "new shipped jobs need five-language examples");
    }

    #[test]
    fn expanded_catalog_specializations_do_not_fill_the_kit_alphabetically() {
        let items = crate::tools::catalog();
        for (request, expected, unrelated) in [
            ("Design the database schema", "pm-sql-queries", vec!["animate", "gsap-core", "pm-cohort-analysis", "pm-metrics-dashboard"]),
            ("Summarize the meeting decisions", "pm-summarize-meeting", vec!["grill-me", "create-prd", "pm-interview-script"]),
            ("Write a PRD with the product requirements", "create-prd", vec!["market-sizing", "grill-me", "pm-release-notes"]),
            ("Animate the menu with GSAP", "gsap-core", vec!["pm-sql-queries", "game-audio-design"]),
            (
                "Add Flutter widget tests",
                "flutter-add-widget-test",
                vec!["flutter-apply-architecture-best-practices", "swift-concurrency", "break-ui"],
            ),
            ("Run a SEO audit of our website", "marketing-seo-audit", vec!["copywriting", "pricing", "marketing-events"]),
            (
                "Create a Minecraft mod with Fabric",
                "minecraft-modding",
                vec!["minecraft-server-admin", "godot-gdscript", "unity-csharp-scripting"],
            ),
            ("Design a save system for my game", "game-save-systems", vec!["game-audio-design", "game-level-design", "game-card-design"]),
        ] {
            let jobs = labels(request, None).0;
            let stacks = stack_labels(request, None);
            let matched: BTreeSet<_> = items
                .iter()
                .filter(|item| jobs.contains(&item.job) && matches_task(item, request, &stacks))
                .map(|item| item.id.as_str())
                .collect();
            assert!(matched.contains(expected), "{request}: {matched:?}");
            for id in unrelated {
                assert!(!matched.contains(id), "{request}: unrelated {id} in {matched:?}");
            }
        }
    }

    #[test]
    fn punctuated_stack_names_keep_their_identity() {
        for (request, id, stack) in [
            ("Improve my React-Native app", "react-native-best-practices", "react-native"),
            ("Improve lighting in Three.js", "threejs-materials-lighting", "threejs"),
        ] {
            let item = crate::tools::find(id).unwrap();
            let stacks = stack_labels(request, None);
            assert!(stacks.contains(stack));
            assert!(labels(request, None).0.contains(&item.job));
            assert!(matches_task(item, request, &stacks));
        }
    }
}
