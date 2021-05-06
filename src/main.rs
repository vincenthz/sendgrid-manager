use anyhow::bail;
use clap::{App, Arg};

use console::{style, Emoji};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use futures::future::join_all;
use sendgrid::template;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

mod manage;

static LOOKING_GLASS: Emoji<'_, '_> = Emoji("🔍  ", "");
static SPARKLE: Emoji<'_, '_> = Emoji("✨ ", ":-)");
static BAD: Emoji<'_, '_> = Emoji("❌ ", "BAD");
static OK: Emoji<'_, '_> = Emoji("✅ ", "OK");
static DOWNLOAD: Emoji<'_, '_> = Emoji("🌍 ", "Download");

fn spinner_style() -> ProgressStyle {
    ProgressStyle::default_spinner()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
        .template("{prefix:.bold.dim} {spinner} {wide_msg}")
}

pub fn read_all_templates<P: AsRef<Path>>(dir: P) -> HashMap<String, manage::Template> {
    let p: &Path = dir.as_ref();

    let mut templates = HashMap::new();
    if let Ok(entries) = p.read_dir() {
        for entry in entries {
            if let Ok(entry) = entry {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let path = entry.path();
                        let ext = path.extension();
                        if ext.map(|e| e.to_str()) == Some(Some("mailtemplate")) {
                            let r = manage::read_template(path);
                            if let Ok(Ok(template)) = r {
                                templates.insert(template.name.clone(), template);
                            } else {
                                println!("{:?} {:?}", entry, r.err())
                            }
                        }
                    }
                }
            }
        }
    }
    templates
}

fn main() -> anyhow::Result<()> {
    const ARG_API_KEY: &str = "API-KEY";
    const ARG_DIR: &str = "DIR";

    const SUBCMD_SYNC_TO_DIR: &str = "sync-to-dir";
    const SUBCMD_CHECK: &str = "check";

    const SUBCMDS: &[&str] = &[SUBCMD_SYNC_TO_DIR, SUBCMD_CHECK];

    let app = App::new("sendgrid-manager")
        .version("0.1")
        .author("HexDev")
        .about("sendgrid manager")
        .arg(
            Arg::new(ARG_API_KEY)
                .short('k')
                .long("key")
                .value_name("API-KEY")
                .about("Set the API key to use to communicate to sendgrid")
                .takes_value(true),
        )
        .subcommand(
            App::new(SUBCMD_SYNC_TO_DIR)
                .about("synchronise the templates on sendgrid to a local directory")
                .arg(
                    Arg::new(ARG_DIR)
                        .takes_value(true)
                        .required(true)
                        .value_name("DIR")
                        .about("directory where the template will be stored"),
                ),
        )
        .subcommand(
            App::new(SUBCMD_CHECK)
                .about("check the local directory templates against the one of the sendgrid")
                .arg(
                    Arg::new(ARG_DIR)
                        .takes_value(true)
                        .required(true)
                        .value_name("DIR")
                        .about("directory where the template are stored"),
                ),
        );
    let matches = app.get_matches();

    let rt = tokio::runtime::Builder::new_current_thread()
        .worker_threads(4)
        .enable_time()
        .enable_io()
        .build()
        .expect("failed to create runtime");

    if let Some(m) = matches.subcommand_matches(SUBCMD_SYNC_TO_DIR) {
        let api_key = matches.value_of(ARG_API_KEY);
        let dir = m.value_of(ARG_DIR).unwrap();
        let dir = PathBuf::from(dir);

        rt.block_on(sync_to_directory(api_key, dir))
    } else if let Some(m) = matches.subcommand_matches(SUBCMD_CHECK) {
        let api_key = matches.value_of(ARG_API_KEY);
        let dir = m.value_of(ARG_DIR).unwrap();
        let dir = PathBuf::from(dir);
        rt.block_on(check_against_local(api_key, dir))
    } else if let Some(name) = matches.subcommand_name() {
        bail!("unknown command {}\nexisting commands: {:?}", name, SUBCMDS)
    } else {
        bail!("no command specified");
    }
}

fn get_api_key(arg_api_key: Option<&str>) -> String {
    match arg_api_key {
        None => {
            let sg_api_key = std::env::var("SG_API_KEY")
                .expect("SG_API_KEY environment to be set or by command line argument");
            sg_api_key
        }
        Some(a) => a.to_string(),
    }
}

async fn list_version_remote_active(
    sg_api_key: &str,
    templates: &[template::Template],
) -> Vec<template::TemplateVersion> {
    let mut found = Vec::new();
    for t in templates {
        if t.versions.len() == 0 {
            continue;
        }

        let active_versions = t
            .versions
            .iter()
            .filter(|v| v.active == 1)
            .collect::<Vec<_>>();
        if active_versions.len() != 1 {
            continue;
        }

        let active = active_versions[0];

        let long: template::TemplateVersion = template::get_version(&sg_api_key, &t.id, &active.id)
            .await
            .expect("sendgrid get version");
        found.push(long)
    }
    found
}

async fn check_against_local(api_key: Option<&str>, dir: PathBuf) -> anyhow::Result<()> {
    let dir = dir.as_path();
    let sg_api_key = get_api_key(api_key);

    let local_templates = read_all_templates(dir);

    println!(
        "{} local templates found in {}",
        ansi_term::Color::Yellow.paint(format!("{}", local_templates.len())),
        dir.to_str().expect("valid utf8 dir")
    );

    if local_templates.len() == 0 {
        return Ok(());
    }

    let remote_templates = template::list(&sg_api_key)
        .await
        .expect("sendgrid list result");

    println!(
        "{} remote templates found",
        ansi_term::Color::Yellow.paint(format!("{}", remote_templates.len()))
    );

    let remote_templates = list_version_remote_active(&sg_api_key, &remote_templates).await;

    for (i, ltmp) in local_templates.values().enumerate() {
        let identifier = format!(
            "[{}/{}]",
            ansi_term::Color::Yellow.paint(format!("{}", i)),
            ansi_term::Color::Yellow.paint(format!("{}", local_templates.len())),
        );
        match remote_templates.iter().find(|l| l.name == ltmp.name) {
            None => {
                println!("{} cannot find remote for \"{}\"", identifier, ltmp.name)
            }
            Some(found) => {
                let content_match = &ltmp.html_body == found.html_content.as_deref().unwrap_or("")
                    && &ltmp.plain_body == found.plain_content.as_deref().unwrap_or("");
                println!(
                    "{} found local \"{}\" as remote={} with content matching {}",
                    identifier, ltmp.name, found.template_id, content_match
                )
            }
        }
    }

    Ok(())
}

async fn get_template(
    dir: PathBuf,
    sg_api_key: Arc<Mutex<String>>,
    _i: usize,
    t: sendgrid::template::Template,
    pb: &ProgressBar,
) -> Result<(), std::io::Error> {
    /*
    println!("####################################################################");
    println!("ID         : {}", t.id);
    println!("NAME       : {}", t.name);
    println!("Generation : {:?}", t.generation);
    println!("Updated    : {:?}", t.updated_at);
    println!("Versions   : {}", t.versions.len());
    */
    if t.versions.len() == 0 {
        return Ok(());
    }

    let active_versions = t
        .versions
        .iter()
        .filter(|v| v.active == 1)
        .collect::<Vec<_>>();
    if active_versions.len() == 0 {
        pb.finish_with_message(&format!("{} error: no active version {}", BAD, t.name));
        return Ok(());
    }
    if active_versions.len() > 1 {
        pb.finish_with_message(&format!(
            "{} error: multiple active versions: {}",
            BAD, t.name
        ));
        return Ok(());
    }

    let active = active_versions[0];

    //println!("   * id={}", active.id);
    //println!("     name={}", active.name);
    //println!("     temp={}", active.template_id);
    //println!("     active={}", active.active);

    pb.set_message(&format!("{} waiting {}", DOWNLOAD, t.name));
    let long = {
        loop {
            match sg_api_key.try_lock() {
                Ok(key) => {
                    pb.set_message(&format!("{} fetching {}", DOWNLOAD, t.name));
                    let v = template::get_version(&key, &t.id, &active.id)
                        .await
                        .expect("sendgrid get version");
                    pb.set_message(&format!("{} analysing {}", LOOKING_GLASS, t.name));
                    break v;
                }
                Err(_) => {
                    pb.inc(1);
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await
                }
            }
        }
    };

    let mut filename = PathBuf::from(dir);
    filename.push(format!("{}.mailtemplate", t.id));

    let template = manage::Template {
        name: long.name,
        plain_body: long.plain_content.unwrap(),
        html_body: long.html_content.unwrap(),
    };

    if let Ok(r) = manage::read_template(&filename) {
        if let Ok(r_templ) = r {
            if &r_templ == &template {
                pb.finish_with_message(&format!("{} same template {}", OK, t.name));
                return Ok(());
            }
            let plain_body_diff = r_templ.plain_body != template.plain_body;
            let html_body_diff = r_templ.html_body != template.html_body;

            let mut tmp_filename = filename.clone();
            tmp_filename.set_extension("mailtemplate.tmp");

            //println!("WRITING tmp file {:?}", tmp_filename.as_path().to_str());
            manage::write_template(&tmp_filename, &template).unwrap();
            if plain_body_diff && html_body_diff {
                pb.finish_with_message(&format!(
                    "{} html and plain bodies differs, writing tmp file {}",
                    BAD,
                    tmp_filename.as_path().to_str().unwrap()
                ));
                Ok(())
            } else {
                pb.finish_with_message(&format!(
                    "{} plain-body={} html-body={}. writing tmp file {}",
                    BAD,
                    plain_body_diff,
                    html_body_diff,
                    tmp_filename.as_path().to_str().unwrap()
                ));
                Ok(())
            }
        } else {
            pb.finish_with_message(&format!("{} error while reading template {:?}", BAD, r));
            Ok(())
        }
    } else {
        manage::write_template(filename, &template)?;
        pb.finish_with_message(&format!("{} downloaded new {}", SPARKLE, t.name));
        Ok(())
    }
}

async fn sync_to_directory(api_key: Option<&str>, dir: PathBuf) -> anyhow::Result<()> {
    //    let dir = dir.as_path();
    let sg_api_key = get_api_key(api_key);

    println!(
        "{} {}Gathering list of templates from server...",
        style("* ").bold().dim(),
        LOOKING_GLASS
    );

    let templates = template::list(&sg_api_key)
        .await
        .expect("sendgrid list result");

    let sg_api_key = Arc::new(Mutex::new(sg_api_key));

    let m = MultiProgress::new();

    let nb_templates = templates.len();
    let mut join_handle = Vec::new();
    for (i, t) in templates.into_iter().enumerate() {
        let pb = m.add(ProgressBar::new(3));
        pb.set_style(spinner_style());
        pb.set_prefix(&format!("[{}/{}] {}", i, nb_templates, t.id));
        pb.set_message(&format!("{}", t.name));

        let dir_clone = dir.clone();
        let sg_api_clone: Arc<Mutex<String>> = sg_api_key.clone();
        let handle = tokio::spawn(async move {
            let r = get_template(dir_clone, sg_api_clone, i, t, &pb).await;
            if let Err(e) = r {
                pb.finish_with_message(&format!("thread issue: {}", e));
            }
            ()
        });
        join_handle.push(handle);
    }
    let end = tokio::task::spawn_blocking(move || m.join());

    for r in join_all(join_handle).await {
        r.unwrap()
    }
    end.await.unwrap().unwrap();
    Ok(())
}
