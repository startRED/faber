//! Offline accounting of Faber-owned first-request text; never starts a process or a model.
use super::*;
use crate::claude::{ClaudeLaunch, claude_args};
use crate::codex::{CodexThread, thread_params};
use crate::guide::{Guide, Language};
use crate::helper::Mcp;
use crate::persona::{BIGGER_OFFER, LEADS, PROJECT_DETAILS, PersonaInput, persona};
use serde_json::{Value, json};

const DESCRIPTION: &str = "Diagnose a failing check before changing code.";
const BODY: &str = "---\nname: debug\ndescription: Diagnose a failing check before changing code.\n---\nRead the failure, fix its cause and check the result.\n";

fn delivery(vendor: Vendor, area: Area, kits: bool, permissions: Permissions) -> Delivery {
    let project = area == Area::Project;
    let active = permissions != Permissions::Isolated;
    let items = ["debug", "design"].map(|id| Item {
        id: id.into(),
        cli_name: None,
        job: if id == "debug" { "debugging" } else { "design" }.into(),
        tier: Default::default(),
        license: "MIT".into(),
        repo: "fixture/original".into(),
        commit: "fixture".into(),
        path: id.into(),
        digest: String::new(),
        server: None,
        chat_safe: false,
    });
    let instructions = (active && kits).then(|| if project { BODY.into() } else { chat_instructions(BODY, "debug") });
    Delivery {
        start_card: project.then(|| crate::project::rules::card(&Default::default(), &[], &[])),
        skills: (active && kits).then(|| "E:/context-fixture/catalog/skills".into()),
        plugin: (active && kits && project && vendor == Vendor::Claude).then(|| "E:/context-fixture/catalog".into()),
        skill_ids: Some(if active && kits { vec!["debug".into()] } else { vec![] }),
        prepared: Some(Arc::new(PreparedText {
            instructions,
            project,
            chat: !project,
            items: items.into(),
            description_ids: if active && kits { vec!["debug".into(), "design".into()] } else { vec![] },
            texts: Arc::new(
                [
                    ("debug".into(), (BODY.into(), DESCRIPTION.into())),
                    ("design".into(), ("Design the interface.".into(), "Build an accessible interface.".into())),
                ]
                .into(),
            ),
            ..Default::default()
        })),
        guide: if !active {
            Guide::Off
        } else if project && vendor == Vendor::Claude {
            Guide::Plugin { dir: "E:/context-fixture/guide-plugin".into(), language: Language::En }
        } else {
            Guide::Index { dir: "E:/context-fixture/guide-plugin".into(), language: Language::En }
        },
        ..Default::default()
    }
}

#[test]
fn selected_description_is_not_repeated_but_discovery_and_whole_instructions_stay() {
    let mut kit = delivery(Vendor::Codex, Area::Project, true, Permissions::Ask);
    let index = kit.project_skill_descriptions(Vendor::Codex).unwrap();
    let prompt = format!("{index}\n\n{}", kit.skill_instructions().unwrap());
    assert_eq!(prompt.matches(DESCRIPTION).count(), 1, "the built context must carry this exact description once");
    assert!(index.contains("debug/SKILL.md") && index.contains("Build an accessible interface."));
    assert!(prompt.ends_with(BODY), "never edit or truncate an installed instruction body");
    let prepared = Arc::make_mut(kit.prepared.as_mut().unwrap());
    Arc::make_mut(&mut prepared.texts).get_mut("design").unwrap().1 = DESCRIPTION.into();
    assert!(kit.project_skill_descriptions(Vendor::Codex).unwrap().contains(DESCRIPTION), "an unselected skill keeps its own description");
    // A missing body, a fallback capability and a shortened description are not literal duplicates.
    Arc::make_mut(kit.prepared.as_mut().unwrap()).instructions = None;
    assert!(kit.project_skill_descriptions(Vendor::Codex).unwrap().contains(DESCRIPTION));
    Arc::make_mut(kit.prepared.as_mut().unwrap()).instructions = Some("Different instructions.".into());
    assert!(kit.project_skill_descriptions(Vendor::Codex).unwrap().contains(DESCRIPTION));
    Arc::make_mut(kit.prepared.as_mut().unwrap()).instructions = Some(format!("A quote: {DESCRIPTION}"));
    assert!(
        kit.project_skill_descriptions(Vendor::Codex).unwrap().contains(DESCRIPTION),
        "matching words alone do not establish a loaded skill"
    );
    assert!(kit.project_skill_descriptions(Vendor::Claude).unwrap().contains(DESCRIPTION), "the fallback has no eager body");
    assert!(delivery(Vendor::Codex, Area::Chat, true, Permissions::Ask).project_skill_descriptions(Vendor::Codex).is_none());
    let long = "A longer checked description ".repeat(12);
    let body = format!("---\nname: debug\ndescription: {long}\n---\nKeep these instructions whole.\n");
    let prepared = Arc::make_mut(kit.prepared.as_mut().unwrap());
    Arc::make_mut(&mut prepared.texts).get_mut("debug").unwrap().clone_from(&(body.clone(), long.clone()));
    prepared.instructions = Some(body.clone());
    assert!(kit.project_skill_descriptions(Vendor::Codex).unwrap().contains(&short_description(&long, DESCRIPTION_LIMIT)));
    assert_eq!(kit.skill_instructions().as_deref(), Some(body.as_str()));
}

#[test]
fn standing_context_size_matrix() {
    let mut rows = vec![];
    for vendor in [Vendor::Codex, Vendor::Claude] {
        for (area, role) in
            [(Area::Chat, "plain"), (Area::Chat, "group"), (Area::Project, "owner"), (Area::Project, "member"), (Area::Project, "helper")]
        {
            for permissions in [
                Permissions::Ask,
                Permissions::AcceptEdits,
                Permissions::Plan,
                Permissions::Auto,
                Permissions::FullAccess,
                Permissions::Isolated,
            ] {
                for kits in [false, true] {
                    let project = area == Area::Project;
                    let own = project && role != "helper";
                    let leads = role == "owner";
                    let full_tools = own && (vendor == Vendor::Codex || permissions == Permissions::FullAccess);
                    let helper = if own {
                        crate::helper::Helper::Offers
                    } else if role == "helper" {
                        crate::helper::Helper::Is
                    } else {
                        crate::helper::Helper::Off
                    };
                    let mut delivery = delivery(vendor, area, kits, permissions);
                    if role == "helper" {
                        delivery.start_card = None;
                    }
                    let group = (role == "group").then(|| "Use metric units.".to_string());
                    let tuning = crate::model::Tuning::default();
                    let launch = ClaudeLaunch {
                        temporary: false,
                        area,
                        model: None,
                        effort: None,
                        resume: None,
                        user_hooks: false,
                        permissions,
                        instructions: None,
                        user_name: None,
                        research: None,
                        group_instructions: group.clone(),
                        flags: crate::claude::ClaudeFlags { plugin_dir: true, ..Default::default() },
                        tuning: tuning.clone(),
                        helper,
                        browse: full_tools,
                        conversations: own,
                        leads,
                        verify: full_tools,
                        progress: own,
                        tools: delivery.clone(),
                    };
                    let thread = CodexThread {
                        temporary: false,
                        title: None,
                        cwd: "E:/context-fixture/work".into(),
                        model: None,
                        effort: None,
                        resume: None,
                        instructions: None,
                        user_name: None,
                        research: None,
                        group_instructions: group.clone(),
                        permissions,
                        leave_plan: false,
                        tuning: tuning.clone(),
                        helper,
                        browse: full_tools,
                        conversations: own,
                        leads,
                        verify: full_tools,
                        progress: own,
                    };
                    let (_, params) = thread_params(area, &thread);
                    let args = claude_args(&launch);
                    let prompt = if vendor == Vendor::Codex {
                        params["developerInstructions"].as_str().unwrap()
                    } else {
                        &args[args.iter().position(|a| a == "--append-system-prompt").unwrap() + 1]
                    };
                    let tools = if vendor == Vendor::Codex {
                        params["dynamicTools"].as_array().cloned().unwrap_or_default()
                    } else {
                        let Mcp::Answer(answer) =
                            crate::helper::mcp(&json!({"method":"tools/list","id":1}), crate::claude::mcp_tools(&launch))
                        else {
                            panic!("tool listing")
                        };
                        answer["result"]["tools"].as_array().unwrap().clone()
                    };
                    let index = delivery.project_skill_descriptions(vendor).unwrap_or_default();
                    let body =
                        if vendor == Vendor::Codex || !project { delivery.skill_instructions().unwrap_or_default() } else { String::new() };
                    let guide = delivery.guide.note().unwrap_or_default();
                    let native_discovery = if vendor == Vendor::Claude && project && permissions != Permissions::Isolated {
                        let mut list = vec![json!({"name":"faber-guide:guide", "description":crate::guide::DESCRIPTION})];
                        for (name, body) in crate::guide::FRONTEND_SKILLS {
                            list.push(json!({"name":format!("faber-guide:{name}"), "description":checked_description(body, "")}));
                        }
                        if kits {
                            for (id, (_, description)) in &*delivery.prepared.as_ref().unwrap().texts {
                                list.push(json!({"name":format!("faber:{id}"), "description":description}));
                            }
                        }
                        serde_json::to_string(&list).unwrap()
                    } else {
                        String::new()
                    };
                    let blocks = json!({
                        "persona":persona(&PersonaInput { name: None, tuning: &tuning, research: None }),
                        "project_details":if project { PROJECT_DETAILS } else { "" },
                        "bigger_offer":if own { BIGGER_OFFER } else { "" },
                        "lead":if leads { LEADS } else { "" },
                        "group":group.map(|words| format!("\n\nThe person's instructions for this chat group:\n{words}")).unwrap_or_default(),
                        "start_card":delivery.start_card.as_deref().unwrap_or_default(),
                        "guide_index":guide, "native_skill_discovery":native_discovery,
                        "skill_index":index, "kit_instructions":body,
                        "base":if vendor == Vendor::Codex { params["baseInstructions"].as_str().unwrap_or_default() } else { "" },
                        "tool_definitions":if tools.is_empty() { String::new() } else { serde_json::to_string(&tools).unwrap() },
                    });
                    assert!(prompt.contains(blocks["persona"].as_str().unwrap()));
                    assert_eq!(prompt.contains(PROJECT_DETAILS), project);
                    assert_eq!(prompt.contains(BIGGER_OFFER), own);
                    assert_eq!(prompt.contains(LEADS), leads);
                    assert_eq!(prompt.contains("Use metric units."), role == "group");
                    assert_eq!(prompt.contains("faber-start-card"), own && vendor == Vendor::Claude);
                    assert_eq!(tools.iter().any(|t| t["name"] == "conversation"), own);
                    assert_eq!(tools.iter().any(|t| t["name"] == "offer_bigger"), own);
                    let unavailable: Vec<_> = tools
                        .iter()
                        .filter(|t| t["name"] == "settings" || t["name"] == "judge_creative")
                        .map(|t| t["name"].clone())
                        .collect();
                    if vendor == Vendor::Claude && permissions == Permissions::Isolated {
                        assert!(unavailable.is_empty(), "unavailable isolated definitions: {unavailable:?}");
                    } else if vendor == Vendor::Claude && permissions == Permissions::Plan && own {
                        assert_eq!(unavailable, [json!("judge_creative")], "settings is unavailable in Plan");
                    } else {
                        assert_eq!(unavailable.len(), if own { 2 } else { 0 });
                    }
                    rows.push(json!({"vendor":vendor,"area":area,"role":role,"permissions":format!("{permissions:?}"),"kits":kits,
                        "blocks":blocks,"tools":tools}));
                }
            }
        }
    }
    assert_eq!(rows.len(), 120);
    println!("CONTEXT_SIZE_JSON={}", Value::Array(rows));
    let mut variants = serde_json::Map::new();
    for language in [Language::En, Language::PtBr, Language::Es, Language::Fr, Language::De] {
        variants.insert(format!("guide_text/{}", language.name()), json!(Guide::Text(language).note().unwrap()));
    }
    let mut legacy = delivery(Vendor::Claude, Area::Project, true, Permissions::Ask);
    legacy.without_plugin();
    variants.insert("older_claude/guide_index".into(), json!(legacy.guide.note().unwrap()));
    variants.insert("older_claude/skill_index".into(), json!(legacy.project_skill_descriptions(Vendor::Claude).unwrap()));
    let mut unmatched = delivery(Vendor::Codex, Area::Project, true, Permissions::Ask);
    Arc::make_mut(unmatched.prepared.as_mut().unwrap()).instructions = None;
    unmatched.skill_ids = Some(vec![]);
    variants
        .insert("enabled_no_selected_kit/codex_skill_index".into(), json!(unmatched.project_skill_descriptions(Vendor::Codex).unwrap()));
    variants.insert(
        "enabled_no_selected_kit/claude_skill_index".into(),
        json!(delivery(Vendor::Claude, Area::Project, true, Permissions::Ask).project_skill_descriptions(Vendor::Claude).unwrap()),
    );
    println!("CONTEXT_VARIANTS_JSON={}", Value::Object(variants));
}
