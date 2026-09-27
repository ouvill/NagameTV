//! Translate and commit commentary presentation before any synchronous Qt signal.
use super::{PlayerRust, comment_posting, ffi, status};
use crate::features::comments::session::{History, Update};
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::{pin::Pin, time::Instant};

macro_rules! projection {
    ($($field:ident: $ty:ty => $signal:ident;)+) => {
        struct Projection { $($field: $ty,)+ }
        struct Changes { $($field: bool,)+ }
        impl Projection {
            fn commit(self, state: &mut PlayerRust) -> Changes {
                let changes = Changes { $($field: state.$field != self.$field,)+ };
                $(state.$field = self.$field;)+
                changes
            }
        }
        impl Changes {
            fn notify(self, mut player: Pin<&mut ffi::Player>) {
                $(if self.$field { player.as_mut().$signal(); })+
            }
        }
    };
}
projection! {
    activity_data: QString => activity_data_changed;
    comment_timeline_revision: u32 => comment_timeline_revision_changed;
    comment_cache_bytes: f64 => comment_cache_bytes_changed;
    comment_draft: QString => comment_draft_changed;
    comment_program_title: QString => comment_program_title_changed;
    comment_status: QString => comment_status_changed;
    comment_post_busy: bool => comment_post_busy_changed;
    comment_post_available: bool => comment_post_available_changed;
    comment_post_target: QString => comment_post_target_changed;
    comment_post_status: QString => comment_post_status_changed;
}

pub(super) fn publish(mut player: Pin<&mut ffi::Player>, update: Option<Update>) {
    let (history, activity, timeline_changed) = match update {
        Some(update) => (
            Some(update.history),
            update.activity,
            update.timeline_changed,
        ),
        None => (None, None, false),
    };
    let projection = {
        let state = player.rust();
        let session = &state.commentary;
        let activity_data = match activity {
            Some(Ok(json)) => QString::from(json),
            Some(Err(error)) => {
                tracing::error!(
                    error = &error as &dyn std::error::Error,
                    "Comment activity projection failed"
                );
                QString::from("[]")
            }
            None => state.activity_data.clone(),
        };
        Projection {
            activity_data,
            comment_timeline_revision: state
                .comment_timeline_revision
                .wrapping_add(u32::from(timeline_changed)),
            comment_cache_bytes: session.disk_bytes() as f64,
            comment_draft: QString::from(session.draft()),
            comment_program_title: QString::from(session.program_title()),
            comment_status: status::comment_text(session.status()),
            comment_post_busy: session.posting_busy(),
            comment_post_available: session
                .posting_available(state.network.as_ref(), Instant::now()),
            comment_post_target: QString::from(
                session
                    .posting_channel()
                    .map(|id| format!("NX-Jikkyo · jk{id}"))
                    .unwrap_or_default(),
            ),
            comment_post_status: comment_posting::render(session.posting_status()),
        }
    };
    let changes = projection.commit(&mut player.as_mut().rust_mut());
    // Model callbacks can also read all the newly committed Player properties.
    match history {
        Some(History::Replace(comments)) => {
            player.as_mut().clear_comment_history();
            player.as_mut().history_model().append(comments);
        }
        Some(History::Append(comments)) => player.as_mut().history_model().append(comments),
        None => {}
    }
    changes.notify(player);
}
