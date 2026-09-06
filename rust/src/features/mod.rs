pub mod comments;
pub mod program_info;
mod subscriptions;
pub mod subtitles;

/// Parsed before Qt/GStreamer initialization. CLI explicitly limits available features.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaunchPlan {
    pub subtitles: bool,
    pub epg: bool,
    pub comments: bool,
    pub locked: bool,
}
impl LaunchPlan {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut plan = Self {
            subtitles: false,
            epg: false,
            comments: false,
            locked: false,
        };
        for arg in args {
            if arg == "--features" {
                return Err(
                    "--features=none|subtitles|epg|comments の形式で指定してください".into(),
                );
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
                    "comments" if !plan.comments => plan.comments = true,
                    _ => {
                        return Err(format!(
                            "不正な機能指定: {name}（none / subtitles / epg / comments）"
                        ));
                    }
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
    fn launch_defaults_off_and_explicit_allowlist_is_strict() -> Result<(), String> {
        let parse = |args: &[&str]| LaunchPlan::parse(args.iter().map(|s| s.to_string()));
        assert_eq!(
            parse(&[])?,
            LaunchPlan {
                subtitles: false,
                epg: false,
                comments: false,
                locked: false
            }
        );
        assert_eq!(
            parse(&["--features=none"])?,
            LaunchPlan {
                subtitles: false,
                epg: false,
                comments: false,
                locked: true
            }
        );
        assert!(parse(&["--features=subtitles,epg"])?.epg);
        assert!(matches!(
            parse(&["--features=comments"]),
            Ok(LaunchPlan { comments: true, .. })
        ));
        assert!(parse(&["--features=subtitles,epg,comments"])?.comments);
        for arg in [
            "--features",
            "--features=",
            "--features=none,epg",
            "--features=comments,comments",
            "--features=epg,epg",
        ] {
            assert!(parse(&[arg]).is_err());
        }
        assert!(parse(&["--features=none", "--features=epg"]).is_err());
        Ok(())
    }
}
