//! Parse all application arguments before environment, settings or GUI initialization.
use crate::features::{FeatureSet, LaunchPlan};
use clap::Parser;
use std::ffi::OsString;

#[derive(Debug, Parser)]
#[command(name = "nagametv", version = crate::build_info::INFO.version, about = "ながめTV")]
struct Arguments {
    /// Print build identity as JSON without initializing the application
    #[arg(group = "mode", long)]
    build_info: bool,
    /// Restrict available features and use transient settings
    #[arg(
        group = "mode",
        long,
        require_equals = true,
        value_name = "none|subtitles,epg,comments"
    )]
    features: Option<FeatureSet>,
    // The test runner owns these arguments, including Qt Quick Test switches.
    #[cfg(feature = "native_tests")]
    #[arg(group = "mode", long, hide = true, num_args = 1.., allow_hyphen_values = true)]
    native_tests: Option<Vec<OsString>>,
    #[cfg(feature = "qml_tests")]
    #[arg(group = "mode", long, hide = true)]
    qml_tests: bool,
    #[cfg(feature = "video_item_tests")]
    #[arg(group = "mode", long, hide = true)]
    video_item_tests: bool,
}

#[derive(Debug)]
pub enum Command {
    Launch(LaunchPlan),
    BuildInfo,
    #[cfg(feature = "native_tests")]
    NativeTests(Vec<OsString>),
    #[cfg(feature = "qml_tests")]
    QmlTests,
    #[cfg(feature = "video_item_tests")]
    VideoItemTests,
}

impl Command {
    pub fn parse_from(
        args: impl IntoIterator<Item = impl Into<OsString> + Clone>,
    ) -> Result<Self, clap::Error> {
        let args = Arguments::try_parse_from(args)?;
        #[cfg(feature = "native_tests")]
        if let Some(arguments) = args.native_tests {
            return Ok(Self::NativeTests(arguments));
        }
        #[cfg(feature = "qml_tests")]
        if args.qml_tests {
            return Ok(Self::QmlTests);
        }
        #[cfg(feature = "video_item_tests")]
        if args.video_item_tests {
            return Ok(Self::VideoItemTests);
        }
        if args.build_info {
            return Ok(Self::BuildInfo);
        }
        Ok(Self::Launch(match args.features {
            Some(features) => LaunchPlan::Restricted(features),
            None => LaunchPlan::Preferences,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn parse(args: &[&str]) -> Result<Command, clap::Error> {
        Command::parse_from(std::iter::once("nagametv").chain(args.iter().copied()))
    }

    #[test]
    fn feature_policy_preserves_preferences_and_restricts_experiments() {
        let Command::Launch(normal) = parse(&[]).unwrap() else {
            panic!("launch");
        };
        assert!(!normal.locked());
        assert!(normal.subtitles() && normal.epg());
        assert!(!normal.comments(false));
        assert!(normal.comments(true));
        for (value, expected) in [
            ("none", [false, false, false]),
            ("subtitles,epg", [true, true, false]),
            ("comments", [false, false, true]),
        ] {
            let Command::Launch(plan) = parse(&[&format!("--features={value}")]).unwrap() else {
                panic!("launch");
            };
            assert!(plan.locked());
            assert_eq!(
                [plan.subtitles(), plan.epg(), plan.comments(false)],
                expected
            );
            assert_eq!(plan.comments(false), plan.comments(true));
        }
    }

    #[test]
    fn invalid_arguments_fail_before_startup() {
        for args in [
            vec!["--unknown"],
            vec!["--features"],
            vec!["--features="],
            vec!["--features=none,epg"],
            vec!["--features=epg,epg"],
            vec!["--features=none", "--features=epg"],
            vec!["--build-info", "--features=none"],
            vec!["--build-info", "unexpected"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn information_commands_do_not_require_a_launch_plan() {
        assert!(matches!(parse(&["--build-info"]), Ok(Command::BuildInfo)));
        assert_eq!(
            parse(&["--help"]).unwrap_err().kind(),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            parse(&["--version"]).unwrap_err().kind(),
            ErrorKind::DisplayVersion
        );
        assert_eq!(
            parse(&["-V"]).unwrap_err().kind(),
            ErrorKind::DisplayVersion
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_input_is_reported_without_panicking() {
        use std::os::unix::ffi::OsStringExt;
        assert!(
            Command::parse_from([OsString::from("nagametv"), OsString::from_vec(vec![0xff])])
                .is_err()
        );
    }
}
