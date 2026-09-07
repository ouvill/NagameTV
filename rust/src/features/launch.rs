//! Startup feature selection, independent of Qt and feature workers.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("--features=none|subtitles|epg|comments の形式で指定してください")]
    MissingEquals,
    #[error("--features は一度だけ指定してください")]
    RepeatedOption,
    #[error("不正な機能指定: {0}（none / subtitles / epg / comments）")]
    InvalidFeature(String),
}

/// Parsed before Qt/GStreamer initialization. CLI explicitly limits available features.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaunchPlan {
    pub subtitles: bool,
    pub epg: bool,
    pub comments: bool,
    pub locked: bool,
}
impl LaunchPlan {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, ParseError> {
        let mut plan = Self {
            subtitles: false,
            epg: false,
            comments: false,
            locked: false,
        };
        for arg in args {
            if arg == "--features" {
                return Err(ParseError::MissingEquals);
            }
            let Some(value) = arg.strip_prefix("--features=") else {
                continue;
            };
            if plan.locked {
                return Err(ParseError::RepeatedOption);
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
                        return Err(ParseError::InvalidFeature(name.to_owned()));
                    }
                }
            }
        }
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_defaults_off_and_explicit_allowlist_is_strict() -> Result<(), ParseError> {
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
        assert_eq!(parse(&["--features"]), Err(ParseError::MissingEquals));
        for (arg, name) in [
            ("--features=", ""),
            ("--features=none,epg", "none"),
            ("--features=comments,comments", "comments"),
            ("--features=epg,epg", "epg"),
        ] {
            assert_eq!(
                parse(&[arg]),
                Err(ParseError::InvalidFeature(name.to_owned()))
            );
        }
        assert_eq!(
            parse(&["--features=none", "--features=epg"]),
            Err(ParseError::RepeatedOption)
        );
        Ok(())
    }
}
