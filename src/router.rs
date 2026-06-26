use axum::Router;

use crate::state::SharedState;
use crate::modules::permission;
use crate::modules::media;
use crate::modules::audit;
use crate::modules::jobs;
use crate::modules::identity;
use crate::modules::profile;
use crate::modules::relationship;
use crate::modules::content;
use crate::modules::search;
use crate::modules::communication;
use crate::modules::trust;
use crate::modules::settings;
use crate::modules::organization;
use crate::modules::admin;
use crate::modules::localization;
use crate::modules::consent;
use crate::modules::events;
use crate::modules::analytics;
use crate::modules::recommendation;
use crate::modules::flags;

pub fn api_routes() -> Router<SharedState> {
    Router::new()
        .nest("/permission", permission::routes())
        .nest("/media", media::routes())
        .nest("/audit", audit::routes())
        .nest("/jobs", jobs::routes())
        .nest("/auth", identity::routes())
        .nest("/profile", profile::routes())
        .nest("/network", relationship::routes())
        .nest("/search", search::routes())
        .nest("/communications", communication::routes())
        .nest("/trust", trust::routes())
        .nest("/settings", settings::routes())
        .nest("/org", organization::routes())
        .nest("/admin", admin::routes())
        .nest("/localization", localization::routes())
        .nest("/consent", consent::routes())
        .nest("/events", events::routes())
        .nest("/analytics", analytics::routes())
        .nest("/recommendation", recommendation::routes())
        .nest("/flags", flags::routes())
        .nest("", content::routes())
}
