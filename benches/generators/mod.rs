// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use std::collections::HashMap;

use anyhow::Error;
use serde_derive::{Deserialize, Serialize};
use tinytemplate::TinyTemplate;

#[derive(Serialize, Deserialize)]
pub(crate) enum RecordTemplate {
    JSON(JSON),
    NGINXAccess(NGINXAccess),
    Qmail(Qmail),
    Sendmail(Sendmail),
    SlowQuery(SlowQuery),
    Syslog(Syslog),
}

#[derive(Serialize, Deserialize)]
pub(crate) struct JSON {
    pub event_type: String,
    pub callsite: String,
    pub app_name: String,
    pub headers: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct NGINXAccess {
    pub ts: String,
    pub client: String,
    pub method: String,
    pub status: usize,
    pub bytes: usize,
    pub path: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Qmail;

#[derive(Serialize, Deserialize)]
pub(crate) struct Sendmail {
    pub ts: String,
    pub remote: String,
    pub status: usize,
    pub message: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SlowQuery {
    pub ts: String,
    pub db: String,
    pub op: String,
    pub duration: String,
    pub index: String,
    pub scanned: usize,
    pub found: usize,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Syslog {
    pub ts: String,
    pub facility: String,
    pub severity: String,
}

const NGINX_TEMPLATE: &str = "{ foo }";
const QMAIL_TEMPLATE: &str = "{ foo }";
const SENDMAIL_TEMPLATE: &str =
    "{ ts } Sent to { remote } with status: { status }, remote said { message }";
const SLOW_QUERY_TEMPLATE: &str = "{ foo }";
const SYSLOG_TEMPLATE: &str = "{ foo }";

pub(crate) struct LogGenerator<'a> {
    tiny: TinyTemplate<'a>,
}

impl<'a> LogGenerator<'_> {
    pub fn new() -> Result<Self, Error> {
        let mut tt = TinyTemplate::new();
        tt.add_template("nginx", NGINX_TEMPLATE)?;
        tt.add_template("qmail", QMAIL_TEMPLATE)?;
        tt.add_template("sendmail", SENDMAIL_TEMPLATE)?;
        tt.add_template("slowquery", SLOW_QUERY_TEMPLATE)?;
        tt.add_template("syslog", SYSLOG_TEMPLATE)?;
        Ok(Self { tiny: tt })
    }

    pub(crate) fn make_record(&self, template: RecordTemplate) -> String {
        match template {
            RecordTemplate::JSON(j) => serde_json::to_string(&j).expect(""),
            RecordTemplate::NGINXAccess(n) => self.tiny.render("nginx", &n).expect(""),
            RecordTemplate::Qmail(q) => self.tiny.render("qmail", &q).expect(""),
            RecordTemplate::Sendmail(s) => self.tiny.render("sendmail", &s).expect(""),
            RecordTemplate::SlowQuery(sq) => self.tiny.render("slowquery", &sq).expect(""),
            RecordTemplate::Syslog(s) => self.tiny.render("syslog", &s).expect(""),
        }
    }
}
