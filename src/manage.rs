use std::path::Path;
use std::collections::HashMap;
use thiserror::*;
use std::fs::File;

pub fn write_template<P: AsRef<Path>>(path: P, template: &Template) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = File::create(path)?;
    file.write_all(TEMPLATE_HEADER.as_bytes())?;
    file.write_all(b"\n")?;
    file.write_all(template.name.as_bytes())?;
    file.write_all(b"######\n")?;
    file.write_all(template.plain_body.as_bytes())?;
    if !template.plain_body.ends_with("\n") {
        file.write_all(b"\n")?;
    }
    file.write_all(b"######\n")?;
    file.write_all(template.html_body.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

pub fn read_template<P: AsRef<Path>>(path: P) -> std::io::Result<Result<Template, TemplateError>> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_template(&content))
}

#[derive(Debug, Clone, Error)]
pub enum TemplateError {
    #[error("header format invalid")]
    HeaderInvalidFormat,
    #[error("header start invalid")]
    HeaderInvalidStart,
    #[error("header name invalid")]
    HeaderInvalidName,
    #[error("header end invalid")]
    HeaderInvalidEnd,
    #[error("body unfinished plain body")]
    BodyPlainUnfinished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub plain_body: String,
    pub html_body: String,
}

const TEMPLATE_HEADER: &str = "SENDGRID-TEMPLATE";
const TEMPLATE_HEADER_SEP: &str = "######";

pub fn parse_template(content: &str) -> Result<Template, TemplateError> {
    let v: Vec<&str> = content.splitn(4, "\n").collect();
    if v.len() != 4 {
        return Err(TemplateError::HeaderInvalidFormat);
    }

    if v[0] != TEMPLATE_HEADER {
        return Err(TemplateError::HeaderInvalidStart);
    }

    if v[1].is_empty() {
        return Err(TemplateError::HeaderInvalidName);
    }

    let name = v[1].to_string();

    if v[2] != TEMPLATE_HEADER_SEP {
        return Err(TemplateError::HeaderInvalidEnd);
    }

    let all_bodies = v[3];

    let mut lines = all_bodies.lines();

    let mut plain_body = String::new();

    loop {
        match lines.next() {
            None => return Err(TemplateError::BodyPlainUnfinished),
            Some(l) if l == TEMPLATE_HEADER_SEP => {
                break;
            }
            Some(l) => {
                if !plain_body.is_empty() {
                    plain_body.push_str("\n");
                }
                plain_body.push_str(l);
            }
        }
    }

    let mut html_body = String::new();
    for (i, l) in lines.enumerate() {
        if i > 0 {
            html_body.push_str("\n");
        }
        html_body.push_str(l);
    }

    Ok(Template { name, plain_body, html_body })
}
