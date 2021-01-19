use std::fs::File;
use std::path::Path;
use thiserror::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub name: String,
    pub plain_body: String,
    pub html_body: String,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("header format invalid")]
    HeaderInvalidFormat,
    #[error("header start invalid {0}")]
    HeaderInvalidStart(String),
    #[error("header name invalid")]
    HeaderInvalidName,
    #[error("header end invalid {0}")]
    HeaderInvalidEnd(String),
    #[error("body unfinished plain body")]
    BodyPlainUnfinished,
}

const TEMPLATE_HEADER: &str = "SENDGRID-TEMPLATE";
const TEMPLATE_HEADER_SEP: &str = "######";

impl Template {
    pub fn write_to<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        out.write_all(TEMPLATE_HEADER.as_bytes())?;
        out.write_all(b"\n")?;
        out.write_all(self.name.as_bytes())?;
        out.write_all(b"\n")?;
        out.write_all(TEMPLATE_HEADER_SEP.as_bytes())?;
        out.write_all(b"\n")?;
        out.write_all(self.plain_body.as_bytes())?;
        if !self.plain_body.ends_with("\n") {
            out.write_all(b"\n")?;
        }
        out.write_all(TEMPLATE_HEADER_SEP.as_bytes())?;
        out.write_all(b"\n")?;
        out.write_all(self.html_body.as_bytes())?;
        out.write_all(b"\n")?;
        Ok(())
    }

    pub fn parse(content: &str) -> Result<Self, TemplateError> {
        let v: Vec<&str> = content.splitn(4, "\n").collect();
        if v.len() != 4 {
            return Err(TemplateError::HeaderInvalidFormat);
        }

        if v[0] != TEMPLATE_HEADER {
            return Err(TemplateError::HeaderInvalidStart(v[0].to_string()));
        }

        if v[1].is_empty() {
            return Err(TemplateError::HeaderInvalidName);
        }

        let name = v[1].to_string();

        if v[2] != TEMPLATE_HEADER_SEP {
            return Err(TemplateError::HeaderInvalidEnd(v[2].to_string()));
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

        Ok(Template {
            name,
            plain_body,
            html_body,
        })
    }
}

pub fn write_template<P: AsRef<Path>>(path: P, template: &Template) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    template.write_to(&mut file)?;
    Ok(())
}

pub fn read_template<P: AsRef<Path>>(path: P) -> std::io::Result<Result<Template, TemplateError>> {
    let content = std::fs::read_to_string(path)?;
    Ok(Template::parse(&content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let template = Template {
            name: "abc".to_string(),
            plain_body: "this is a plain body on\nmulti lines".to_string(),
            html_body: "this is <b>html body</b> with some kind of <u>escape</u>".to_string(),
        };

        let mut out = Vec::new();
        template.write_to(&mut out).unwrap();

        let s = String::from_utf8(out).unwrap();
        assert_eq!(Ok(template), Template::parse(&s))
    }
}
