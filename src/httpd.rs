use crate::list::{ls, FileInfo, LsRes};
use crate::send::{ResponseFuture, Sender};
use crate::utils::parse_range;
use hyper::header::{HeaderMap, HeaderValue};
use hyper::rt::{self, Future};
use hyper::service::service_fn;
use hyper::{Body, Request, Server};
use log::{error, info};
use percent_encoding::{
    percent_decode, utf8_percent_encode, DEFAULT_ENCODE_SET,
};
use std::net::SocketAddr;
use std::sync::Arc;

pub struct D {
    path: String,
    sender: Sender,
}

impl D {
    pub fn new(path: &str) -> D {
        D {
            path: path.trim_end_matches('/').to_string(),
            sender: Sender::new(),
        }
    }

    fn render_dir(&self, dir: &str, files: &[FileInfo]) -> ResponseFuture {
        let dir = dir.to_owned() + "/";
        let sl: Vec<String> = files
            .iter()
            .map(|info| {
                let path = &utf8_percent_encode(
                    &info.0[self.path.len()..],
                    DEFAULT_ENCODE_SET,
                )
                .to_string();
                // also remove "/" before file name
                let mut name = info.0[dir.len()..].to_string();
                if info.1.is_dir() {
                    name += "/";
                }

                String::from("<a href=") + path + ">" + &name + "</a></br>"
            })
            .collect();

        let current = dir[self.path.len()..].to_string();
        let title = format!("Index of {}", current);
        let parent = if current == "/" {
            "".to_owned()
        } else {
            format!("<a href={}..>..</a>", &current)
        };

        self.sender.send_string(&format!(
            "<!doctype html>
<html lang=\"en\">
<head>
<meta charset=\"utf-8\">
<meta name=\"viewport\" content=\"width=device-width\">
<title>{}</title>
</head>
<body>
<h1>{}</h1>
<div>
{}</br>{}
</div>
</body>
</html>",
            &title,
            &title,
            &parent,
            &sl.concat()
        ))
    }

    fn render_file(
        &self,
        file_info: &FileInfo,
        req_headers: &HeaderMap<HeaderValue>,
    ) -> ResponseFuture {
        let range = match req_headers.get("range") {
            Some(r) => match r.to_str() {
                Ok(s) => parse_range(s),
                _ => None,
            },
            _ => None,
        };
        self.sender.send_file(&file_info, range)
    }

    fn render(
        &self,
        rel_path: &str,
        req_headers: &HeaderMap<HeaderValue>,
    ) -> ResponseFuture {
        let file_path = self.path.to_string() + rel_path;
        let res = ls(&file_path);

        match res {
            Ok(LsRes::Dir(ref files)) => self.render_dir(&file_path, files),
            Ok(LsRes::File(ref file_info)) => {
                self.render_file(&file_info, req_headers)
            }
            _ => self.sender.send_500(),
        }
    }
}

pub fn start(addr: &SocketAddr, path: &str) {
    let d = Arc::new(D::new(path));

    let server = Server::bind(addr)
        .serve(move || {
            let d = d.clone();

            service_fn(move |req: Request<Body>| {
                let req_headers = req.headers();
                let dec =
                    percent_decode(req.uri().path().as_bytes()).decode_utf8();
                let path = match dec {
                    // need remove "/" at the end of directory
                    Ok(ref p) => p.trim_end_matches('/'),
                    _ => {
                        return d.sender.send_404();
                    }
                };

                if path == "/favicon.ico" {
                    return d.sender.send_404();
                }

                info!("{}", path);
                d.render(path, req_headers)
            })
        })
        .map_err(|e| error!("server error: {}", e));

    info!("Listening on http://{}", addr);
    rt::run(server);
}
