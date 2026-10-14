use serde_json::{Value, json};
use std::io;
#[cfg(windows)]
use std::path::Path;
#[cfg(windows)]
use std::time::{Duration, Instant};
#[cfg(windows)]
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

fn empty_loadout(observed: &Value) -> io::Result<()> {
    if observed["data"].as_array().is_none_or(|rows| !rows.is_empty()) || !observed["nextCursor"].is_null() {
        return Err(io::Error::other("session loadout differs from the empty role loadout"));
    }
    Ok(())
}

#[cfg(windows)]
async fn packet(p: &mut faber_engine::proc::Proc, value: Value) -> io::Result<()> {
    tokio::time::timeout(Duration::from_secs(2), async {
        p.stdin.write_all(format!("{value}\n").as_bytes()).await?;
        p.stdin.flush().await
    })
    .await
    .map_err(|_| io::Error::other("protocol write timeout"))?
}

#[cfg(windows)]
async fn reply(lines: &mut tokio::io::Lines<BufReader<Box<dyn tokio::io::AsyncRead + Send + Unpin>>>, id: u64) -> io::Result<Value> {
    tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(line) = lines.next_line().await? {
            if line.len() > 4 * 1024 * 1024 {
                return Err(io::Error::other("oversized protocol message"));
            }
            let msg: Value = serde_json::from_str(&line)?;
            if msg["id"].as_u64() == Some(id) {
                if !msg["error"].is_null() {
                    return Err(io::Error::other("protocol request refused"));
                }
                return Ok(msg["result"].clone());
            }
        }
        Err(io::Error::other("protocol EOF"))
    })
    .await
    .map_err(|_| io::Error::other("protocol timeout"))?
}

/// Observe the installed role loadout without requesting a model turn or reading configuration.
#[cfg(windows)]
pub async fn run(data: &Path) -> io::Result<()> {
    use faber_engine::budget::Billing;
    use faber_engine::project::Role;
    use std::io::Write;
    let evidence = data.join("live-denial.json");
    let mut receipt = std::fs::OpenOptions::new().create_new(true).write(true).open(&evidence)?;
    let measured = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_secs();
    let root = std::path::PathBuf::from(std::env::var("FABER_QUALIFICATION_WORKSPACE").map_err(io::Error::other)?);
    // This folder is created by this probe, never the owner's checkout or a CLI's auth folder.
    std::fs::create_dir_all(&root)?;
    let work = root.join("lease");
    std::fs::create_dir_all(&work)?;
    let marker = super::random_marker();
    let canary = root.join("outside-marker.txt");
    std::fs::write(&canary, &marker)?;
    let cli = faber_engine::cli::find_cli("codex").ok_or_else(|| io::Error::other("CLI unavailable"))?;
    let identity = super::fingerprint(Path::new(&cli.command))?;
    let args: Vec<_> = cli.prefix.iter().cloned().chain(faber_engine::codex::project_codex_args(true)).collect();
    let mut p = faber_engine::proc::real_spawner()(&cli.command, &args, &work)?;
    if let Some(stderr) = p.stderr.take() {
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut stderr.take(65536), &mut tokio::io::sink()).await;
        });
    }
    let stdout = std::mem::replace(&mut p.stdout, Box::new(tokio::io::empty()));
    let mut lines = BufReader::new(stdout).lines();
    let mut row = json!({"schema":1,"measuredAt":measured,"vendor":"codex","binaryHash":identity,
        "qualificationPassed":false,"modelTurns":0,"state":"running","expectedMcpServers":[],
        "measuredStopReserveMicros":null,"nativeTools":"unobserved","plugins":"unobserved"});
    let result: io::Result<()> = async {
        packet(&mut p, json!({"id":1,"method":"initialize","params":{"clientInfo":{"name":"faber_qualification","version":"0.1.0"}}})).await?;
        reply(&mut lines,1).await?;
        packet(&mut p,json!({"method":"initialized"})).await?;
        packet(&mut p,json!({"id":2,"method":"account/read","params":{"refreshToken":false}})).await?;
        let account = reply(&mut lines,2).await?;
        let signals = faber_engine::signals::codex_account(&json!({"params":{"authMode":account["account"]["type"],"planType":account["account"]["planType"]}}));
        row["billing"] = json!(if signals.billing == Some(Billing::Subscription) { "subscription" } else { "unknownOrPaid" });
        if signals.billing != Some(Billing::Subscription) { return Err(io::Error::other("billing unknown or paid")); }
        packet(&mut p,json!({"id":3,"method":"account/rateLimits/read"})).await?;
        let limits = reply(&mut lines,3).await?;
        let signals = faber_engine::signals::codex_signals(&json!({"params":{"rateLimits":limits["rateLimits"]}}));
        row["paidFallback"] = json!(signals.paid_fallback);
        row["quota"] = json!({"primary":limits["rateLimits"]["primary"],"secondary":limits["rateLimits"]["secondary"]});
        if signals.paid_fallback != Some(false) { return Err(io::Error::other("credit fallback not disabled")); }
        let (_,mut params) = faber_engine::codex::project_thread_params(&faber_engine::codex::CodexThread {
            cwd:work.clone(),model:Some("gpt-6-luna".into()),effort:Some("low".into()),resume:None,instructions:None,user_name:None
        },Role::Writer).map_err(|_|io::Error::other("invalid invocation"))?;
        params["ephemeral"]=true.into();
        packet(&mut p,json!({"id":4,"method":"thread/start","params":params})).await?;
        let thread = reply(&mut lines,4).await?;
        row["effectivePolicy"]=json!({"model":thread["model"],"provider":thread["modelProvider"],"effort":thread["reasoningEffort"],"sandbox":thread["sandbox"],"approvalPolicy":thread["approvalPolicy"]});
        if thread["model"]!="gpt-6-luna" || thread["reasoningEffort"]!="low" || thread["modelProvider"]!="openai" { return Err(io::Error::other("model pin not acknowledged")); }
        let id = thread["thread"]["id"].as_str().ok_or_else(||io::Error::other("thread identity missing"))?;
        if std::env::var("FABER_QUALIFICATION_PROBE").as_deref()==Ok("--live-loadout") {
            let mut loadout=serde_json::Map::new();
            for (number,method,params) in [
                (10,"hooks/list",json!({"cwds":[work]})),
                (11,"mcpServerStatus/list",json!({"threadId":id,"limit":100,"detail":"toolsAndAuthOnly"})),
                (12,"plugin/installed",json!({"cwds":[work]})),
            ] {
                row["currentLoadoutQuery"]=json!(method);
                packet(&mut p,json!({"id":number,"method":method,"params":params})).await?;
                let observed=reply(&mut lines,number).await?;
                if method=="mcpServerStatus/list" {
                    row["mcpMetadata"]=json!(observed["data"].as_array().map(|rows|rows.iter().map(|r|json!({"name":r["name"],"toolCount":r["tools"].as_object().map(serde_json::Map::len)})).collect::<Vec<_>>()));
                    if empty_loadout(&observed).is_err() {
                        row["stopRule"]=1.into();
                        return Err(io::Error::other("session loadout differs from the empty role loadout"));
                    }
                }
                if method=="hooks/list" {
                    row["hookMetadata"]=json!(observed["data"].as_array().map(|rows|rows.iter().map(|r|json!({"keys":r.as_object().map(|o|o.keys().collect::<Vec<_>>()),"hookCount":r["hooks"].as_array().map(Vec::len)})).collect::<Vec<_>>()));
                }
                // Only structure/counts are captured; no hook command, config value or tool result is recorded.
                let shapes:serde_json::Map<_,_>=observed.as_object().into_iter().flatten().map(|(key,value)|
                    (key.clone(),json!({"arrayLength":value.as_array().map(Vec::len),"objectKeys":value.as_object().map(|o|o.keys().collect::<Vec<_>>())}))).collect();
                loadout.insert(method.into(),json!(shapes));
                row["loadoutQueries"]=json!(loadout);
            }
            row["loadoutQueries"]=json!(loadout);
            row["state"]="observed".into();
            return Ok(());
        }
        Err(io::Error::other("unsupported no-model observation"))
    }.await;
    let killed = Instant::now();
    if let Some(child) = p.child.as_mut() {
        child.start_kill()?;
        tokio::time::timeout(Duration::from_secs(3), child.wait()).await.map_err(|_| io::Error::other("owned tree kill timeout"))??;
    }
    row["killWaitMs"] = json!(killed.elapsed().as_millis() as u64);
    if let Err(error) = &result {
        row["state"] = "stopped".into();
        row["failure"] = json!(error.to_string());
    }
    receipt.write_all(&serde_json::to_vec_pretty(&row)?)?;
    receipt.sync_all()?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inherited_loadout_and_missing_observations_cannot_pass() {
        assert!(empty_loadout(&json!({"data":[],"nextCursor":null})).is_ok());
        for observed in [json!({}), json!({"data":[{"name":"inherited"}]}), json!({"data":[],"nextCursor":"more"})] {
            assert!(empty_loadout(&observed).is_err());
        }
    }
}
