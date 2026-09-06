pub mod program_info;
mod subscriptions;
pub mod subtitles;

/// Parsed before Qt/GStreamer initialization. CLI explicitly limits available features.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaunchPlan {
    pub subtitles: bool,
    pub epg: bool,
    pub locked: bool,
}
impl LaunchPlan {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut plan = Self {
            subtitles: false,
            epg: false,
            locked: false,
        };
        for arg in args {
            if arg == "--features" {
                return Err("--features=none|subtitles|epg の形式で指定してください".into());
            }
            let Some(value) = arg.strip_prefix("--features=") else {
                continue;
            };
            if plan.locked {
                return Err("--features は一度だけ指定してください".into());
            }
            plan.locked = true;
            if value == "none" {
                continue;
            }
            for name in value.split(',') {
                match name {
                    "subtitles" if !plan.subtitles => plan.subtitles = true,
                    "epg" if !plan.epg => plan.epg = true,
                    _ => return Err(format!("不正な機能指定: {name}（none / subtitles / epg）")),
                }
            }
        }
        Ok(plan)
    }
}
pub static PLAN: std::sync::OnceLock<LaunchPlan> = std::sync::OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_defaults_off_and_explicit_allowlist_is_strict() {
        let parse = |args: &[&str]| LaunchPlan::parse(args.iter().map(|s| s.to_string()));
        assert_eq!(
            parse(&[]).unwrap(),
            LaunchPlan {
                subtitles: false,
                epg: false,
                locked: false
            }
        );
        assert_eq!(
            parse(&["--features=none"]).unwrap(),
            LaunchPlan {
                subtitles: false,
                epg: false,
                locked: true
            }
        );
        assert!(parse(&["--features=subtitles,epg"]).unwrap().epg);
        for arg in [
            "--features",
            "--features=",
            "--features=none,epg",
            "--features=comments",
            "--features=epg,epg",
        ] {
            assert!(parse(&[arg]).is_err());
        }
        assert!(parse(&["--features=none", "--features=epg"]).is_err());
    }
}
