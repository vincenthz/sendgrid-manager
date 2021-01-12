use sendgrid::template;
use std::path::Path;
use std::collections::HashMap;
use thiserror::Error;
use std::fs::File;

mod manage;

fn list() {
    
}

pub fn read_all_templates<P: AsRef<Path>>(dir: P) -> HashMap<String, manage::Template> {
    let p: &Path = dir.as_ref();
    assert!(
        !p.is_dir(),
        "load from directory failed: file not supported"
    );

    let mut templates = HashMap::new();
    if let Ok(entries) = p.read_dir() {
        for entry in entries {
            if let Ok(entry) = entry {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let path = entry.path();
                        if path.ends_with(".mailtemplate") {
                            if let Ok(Ok(template)) = manage::read_template(path) {
                                templates.insert(template.name.clone(), template);
                            }
                        }
                    }
                }
            }
        }
    }
    templates
}

#[tokio::main]
async fn main() {
    let sg_api_key = env!("SG_API_KEY");

    let templates = template::list(sg_api_key).await.expect("sendgrid list result");

    for t in templates {
        println!("####################################################################");
        println!("ID         : {}", t.id);
        println!("NAME       : {}", t.name);
        println!("Generation : {:?}", t.generation);
        println!("Updated    : {:?}", t.updated_at);
        println!("Versions   : {}", t.versions.len());
        if t.versions.len() == 0 {
            continue
        }

        let active_versions = t.versions.iter().filter(|v| v.active == 1).collect::<Vec<_>>();
        if active_versions.len() == 0 {
            println!("**** no active version ****");
            continue;
        }

        if active_versions.len() > 1 {
            println!("**** multiple active versions {} ****", active_versions.len());
            continue;
        }

        let active = active_versions[0];

        println!("   * id={}", active.id);
        println!("     name={}", active.name);
        println!("     temp={}", active.template_id);
        println!("     active={}", active.active);
        
        let long = template::get_version(sg_api_key, &t.id, &active.id).await.expect("sendgrid get version");

        let filename = format!("{}.mailtemplate", t.id);

        let template = manage::Template {
            name: t.name,
            plain_body: long.plain_content.unwrap(),
            html_body: long.html_content.unwrap(),
        };

        //println!("{:?}", long);
        if let Ok(r) = manage::read_template(&filename) {
            if let Ok(r_templ) = r {
                if &r_templ == &template {
                    println!("same template");
                    continue;
                }
                if r_templ.plain_body != template.plain_body {
                    println!("plain body vary");
                }
                if r_templ.html_body != template.html_body {
                    println!("html body vary");
                }
                let tmp_filename = format!("{}.tmp", filename);
                println!("WRITING tmp file {}", tmp_filename);
                manage::write_template(tmp_filename, &template).unwrap();
            } else {
                println!("error while reading template {:?}", r)
            }
            //println!("templ: {}")
        } else {
            manage::write_template(filename, &template).unwrap();
        }
    } 
}
