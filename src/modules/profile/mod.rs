use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post, put, delete},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::extractors::AuthUser;
use crate::state::SharedState;
use crate::error::{ApiError, ApiResult, ok_json, ok_message};
use crate::modules::audit;

// ══════════════════════════════════════════════
// Profile Engine  (Screens 11–28)
// Full Supabase-backed profile management
// ══════════════════════════════════════════════

pub fn routes() -> Router<SharedState> {
    Router::new()
        // Screen 11–12: View profile (own + public)
        .route("/me", get(get_me))
        .route("/:username", get(get_by_username))
        // Screen 13–14: Update basic info / about
        .route("/basic", put(update_basic))
        .route("/about", put(update_about))
        // Screen 15–17: Experience CRUD
        .route("/experience", post(add_experience))
        .route("/experience/:id", put(update_experience).delete(delete_experience))
        // Screen 18–19: Education CRUD
        .route("/education", post(add_education))
        .route("/education/:id", put(update_education).delete(delete_education))
        // Screen 20: Qualifications
        .route("/qualifications", post(add_qualifications))
        .route("/qualifications/:id", put(update_qualifications).delete(delete_qualifications))
        // Screen 21: Licenses
        .route("/licenses", post(add_license))
        .route("/licenses/:id", put(update_license).delete(delete_license))
        // Screen 22: Skills
        .route("/skills", post(add_skills))
        .route("/skills/:id", delete(delete_skill))
        // Screen 23: Certifications
        .route("/certifications", post(add_certification))
        .route("/certifications/:id", put(update_certification).delete(delete_certification))
        // Screen 24–25: Research & Publications
        .route("/research", post(add_research))
        .route("/research/:id", put(update_research).delete(delete_research))
        .route("/publications", post(add_publication))
        .route("/publications/:id", put(update_publication).delete(delete_publication))
        // Screen 26: Achievements
        .route("/achievements", post(add_achievement))
        .route("/achievements/:id", delete(delete_achievement))
        // Screen 27: Profile completion score
        .route("/completion", get(get_completion))
        // Screen 28: Profile analytics
        .route("/analytics", get(get_analytics))
}

// ══════════════════════════════════════════════
// Request / Response types
// ══════════════════════════════════════════════

#[derive(Deserialize)]
struct BasicInfo {
    first_name: Option<String>,
    last_name: Option<String>,
    preferred_name: Option<String>,
    professional_headline: Option<String>,
    country: Option<String>,
    timezone: Option<String>,
}

#[derive(Deserialize)]
struct AboutInfo {
    bio: Option<String>,
    location: Option<String>,
    website: Option<String>,
    cover_photo: Option<String>,
    social_links: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ExperienceInput {
    company: String,
    role: String,
    start_date: String,
    end_date: Option<String>,
    description: Option<String>,
    is_current: Option<bool>,
    location: Option<String>,
}

#[derive(Deserialize)]
struct EducationInput {
    institution: String,
    degree: String,
    field_of_study: String,
    start_date: String,
    end_date: Option<String>,
    description: Option<String>,
    gpa: Option<String>,
}

#[derive(Deserialize)]
struct QualificationInput {
    qualification_name: String,
    institution: String,
    year_obtained: Option<String>,
    field: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct LicenseInput {
    license_name: String,
    issuing_body: String,
    license_number: String,
    issued_date: Option<String>,
    expiry_date: Option<String>,
    verification_status: Option<String>,
}

#[derive(Deserialize)]
struct SkillsInput {
    skills: Vec<SkillItem>,
}

#[derive(Deserialize)]
struct SkillItem {
    skill_name: String,
    category: Option<String>,
    proficiency: Option<String>,
    years_experience: Option<i32>,
}

#[derive(Deserialize)]
struct CertificationInput {
    certification_name: String,
    issuing_organization: String,
    issue_date: Option<String>,
    expiry_date: Option<String>,
    credential_id: Option<String>,
    credential_url: Option<String>,
}

#[derive(Deserialize)]
struct ResearchInput {
    title: String,
    abstract_text: Option<String>,
    publication_date: Option<String>,
    journal: Option<String>,
    doi: Option<String>,
    url: Option<String>,
    co_authors: Option<Vec<String>>,
    category: Option<String>,
}

#[derive(Deserialize)]
struct PublicationInput {
    title: String,
    publication_type: Option<String>,
    journal_or_publisher: Option<String>,
    publication_date: Option<String>,
    doi: Option<String>,
    url: Option<String>,
    co_authors: Option<Vec<String>>,
    abstract_text: Option<String>,
}

#[derive(Deserialize)]
struct AchievementInput {
    title: String,
    description: Option<String>,
    date_obtained: Option<String>,
    category: Option<String>,
    issuing_organization: Option<String>,
}

// ══════════════════════════════════════════════
// Screen 11–12: Get Profile
// ══════════════════════════════════════════════

async fn get_me(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    // Fetch core profile
    let url = format!(
        "{}/rest/v1/profiles?id=eq.{}&select=*",
        state.rest_url(), user_id
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let profiles: Value = res.json().await?;

    let profile = profiles
        .as_array()
        .and_then(|a| a.first().cloned())
        .ok_or_else(|| ApiError::NotFound("Profile not found".into()))?;

    // Fetch experience
    let exp_url = format!(
        "{}/rest/v1/profile_experiences?profile_id=eq.{}&select=*&order=start_date.desc",
        state.rest_url(), user_id
    );
    let exp_res = state.http.get(&exp_url).headers(state.supabase_headers()).send().await?;
    let experiences: Value = exp_res.json().await.unwrap_or(json!([]));

    // Fetch education
    let edu_url = format!(
        "{}/rest/v1/profile_education?profile_id=eq.{}&select=*&order=start_date.desc",
        state.rest_url(), user_id
    );
    let edu_res = state.http.get(&edu_url).headers(state.supabase_headers()).send().await?;
    let education: Value = edu_res.json().await.unwrap_or(json!([]));

    // Fetch skills
    let skill_url = format!(
        "{}/rest/v1/profile_skills?profile_id=eq.{}&select=*",
        state.rest_url(), user_id
    );
    let skill_res = state.http.get(&skill_url).headers(state.supabase_headers()).send().await?;
    let skills: Value = skill_res.json().await.unwrap_or(json!([]));

    // Fetch research
    let research_url = format!(
        "{}/rest/v1/profile_research?profile_id=eq.{}&select=*&order=publication_date.desc",
        state.rest_url(), user_id
    );
    let research_res = state.http.get(&research_url).headers(state.supabase_headers()).send().await?;
    let research: Value = research_res.json().await.unwrap_or(json!([]));

    // Fetch certifications
    let cert_url = format!(
        "{}/rest/v1/profile_certifications?profile_id=eq.{}&select=*",
        state.rest_url(), user_id
    );
    let cert_res = state.http.get(&cert_url).headers(state.supabase_headers()).send().await?;
    let certifications: Value = cert_res.json().await.unwrap_or(json!([]));

    // Fetch licenses
    let lic_url = format!(
        "{}/rest/v1/profile_licenses?profile_id=eq.{}&select=*",
        state.rest_url(), user_id
    );
    let lic_res = state.http.get(&lic_url).headers(state.supabase_headers()).send().await?;
    let licenses: Value = lic_res.json().await.unwrap_or(json!([]));

    // Fetch achievements
    let ach_url = format!(
        "{}/rest/v1/profile_achievements?profile_id=eq.{}&select=*",
        state.rest_url(), user_id
    );
    let ach_res = state.http.get(&ach_url).headers(state.supabase_headers()).send().await?;
    let achievements: Value = ach_res.json().await.unwrap_or(json!([]));

    // Fetch qualifications
    let qual_url = format!(
        "{}/rest/v1/profile_qualifications?profile_id=eq.{}&select=*",
        state.rest_url(), user_id
    );
    let qual_res = state.http.get(&qual_url).headers(state.supabase_headers()).send().await?;
    let qualifications: Value = qual_res.json().await.unwrap_or(json!([]));

    ok_json(json!({
        "profile": profile,
        "experience": experiences,
        "education": education,
        "skills": skills,
        "research": research,
        "certifications": certifications,
        "licenses": licenses,
        "achievements": achievements,
        "qualifications": qualifications,
    }))
}

async fn get_by_username(
    State(state): State<SharedState>,
    Path(username): Path<String>,
) -> ApiResult {
    // Fetch public profile by username
    let url = format!(
        "{}/rest/v1/profiles?username=eq.{}&select=id,full_name,username,professional_headline,bio,location,website,cover_photo,avatar_url,created_at",
        state.rest_url(), username
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let profiles: Value = res.json().await?;

    let profile = profiles
        .as_array()
        .and_then(|a| a.first().cloned())
        .ok_or_else(|| ApiError::NotFound("User not found".into()))?;

    let profile_id = profile["id"].as_str().unwrap_or("");

    // Fetch public experience
    let exp_url = format!(
        "{}/rest/v1/profile_experiences?profile_id=eq.{}&select=id,company,role,start_date,end_date,is_current,location&order=start_date.desc",
        state.rest_url(), profile_id
    );
    let exp_res = state.http.get(&exp_url).headers(state.supabase_headers()).send().await?;
    let experiences: Value = exp_res.json().await.unwrap_or(json!([]));

    // Fetch public education
    let edu_url = format!(
        "{}/rest/v1/profile_education?profile_id=eq.{}&select=id,institution,degree,field_of_study,start_date,end_date&order=start_date.desc",
        state.rest_url(), profile_id
    );
    let edu_res = state.http.get(&edu_url).headers(state.supabase_headers()).send().await?;
    let education: Value = edu_res.json().await.unwrap_or(json!([]));

    // Fetch public skills
    let skill_url = format!(
        "{}/rest/v1/profile_skills?profile_id=eq.{}&select=skill_name,category,proficiency",
        state.rest_url(), profile_id
    );
    let skill_res = state.http.get(&skill_url).headers(state.supabase_headers()).send().await?;
    let skills: Value = skill_res.json().await.unwrap_or(json!([]));

    // Fetch public research
    let research_url = format!(
        "{}/rest/v1/profile_research?profile_id=eq.{}&select=id,title,publication_date,journal,doi,url,category&order=publication_date.desc",
        state.rest_url(), profile_id
    );
    let research_res = state.http.get(&research_url).headers(state.supabase_headers()).send().await?;
    let research: Value = research_res.json().await.unwrap_or(json!([]));

    ok_json(json!({
        "profile": profile,
        "experience": experiences,
        "education": education,
        "skills": skills,
        "research": research,
    }))
}

// ══════════════════════════════════════════════
// Screen 13: Update Basic Info
// ══════════════════════════════════════════════

async fn update_basic(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<BasicInfo>,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    let mut update_fields = serde_json::Map::new();
    if let Some(v) = &payload.first_name { update_fields.insert("first_name".into(), json!(v)); }
    if let Some(v) = &payload.last_name { update_fields.insert("last_name".into(), json!(v)); }
    if let Some(v) = &payload.preferred_name { update_fields.insert("preferred_name".into(), json!(v)); }
    if let Some(v) = &payload.professional_headline { update_fields.insert("professional_headline".into(), json!(v)); }
    if let Some(v) = &payload.country { update_fields.insert("country".into(), json!(v)); }
    if let Some(v) = &payload.timezone { update_fields.insert("timezone".into(), json!(v)); }

    // Rebuild full_name from first + last if both provided
    if let (Some(first), Some(last)) = (&payload.first_name, &payload.last_name) {
        update_fields.insert("full_name".into(), json!(format!("{} {}", first, last)));
    }

    update_fields.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), user_id);
    let res = state.http
        .patch(&url)
        .headers(state.supabase_headers())
        .json(&json!(update_fields))
        .send()
        .await?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        tracing::error!("Profile update error: {}", err);
        return Err(ApiError::Internal("Failed to update profile".into()));
    }

    audit::log_action(user_id, "profile.update_basic", "profiles", user_id).await;
    ok_message("Profile basic info updated successfully")
}

// ══════════════════════════════════════════════
// Screen 14: Update About
// ══════════════════════════════════════════════

async fn update_about(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<AboutInfo>,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    let mut update_fields = serde_json::Map::new();
    if let Some(v) = &payload.bio { update_fields.insert("bio".into(), json!(v)); }
    if let Some(v) = &payload.location { update_fields.insert("location".into(), json!(v)); }
    if let Some(v) = &payload.website { update_fields.insert("website".into(), json!(v)); }
    if let Some(v) = &payload.cover_photo { update_fields.insert("cover_photo".into(), json!(v)); }
    if let Some(v) = &payload.social_links { update_fields.insert("social_links".into(), v.clone()); }
    update_fields.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), user_id);
    let res = state.http
        .patch(&url)
        .headers(state.supabase_headers())
        .json(&json!(update_fields))
        .send()
        .await?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        tracing::error!("Profile about update error: {}", err);
        return Err(ApiError::Internal("Failed to update about".into()));
    }

    audit::log_action(user_id, "profile.update_about", "profiles", user_id).await;
    ok_message("About section updated successfully")
}

// ══════════════════════════════════════════════
// Screen 15–17: Experience CRUD
// ══════════════════════════════════════════════

async fn add_experience(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<ExperienceInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let exp_id = uuid::Uuid::new_v4().to_string();

    let body = json!({
        "id": exp_id,
        "profile_id": user_id,
        "company": payload.company,
        "role": payload.role,
        "start_date": payload.start_date,
        "end_date": payload.end_date,
        "description": payload.description.unwrap_or_default(),
        "is_current": payload.is_current.unwrap_or(false),
        "location": payload.location.unwrap_or_default(),
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let url = format!("{}/rest/v1/profile_experiences", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await?;

    if !res.status().is_success() {
        let err = res.text().await.unwrap_or_default();
        tracing::error!("Add experience error: {}", err);
        return Err(ApiError::Internal("Failed to add experience".into()));
    }

    audit::log_action(user_id, "profile.add_experience", "profile_experiences", &exp_id).await;
    ok_json(json!({ "id": exp_id, "message": "Experience added successfully" }))
}

async fn update_experience(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<ExperienceInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    let mut update_fields = serde_json::Map::new();
    update_fields.insert("company".into(), json!(payload.company));
    update_fields.insert("role".into(), json!(payload.role));
    update_fields.insert("start_date".into(), json!(payload.start_date));
    if let Some(v) = &payload.end_date { update_fields.insert("end_date".into(), json!(v)); }
    if let Some(v) = &payload.description { update_fields.insert("description".into(), json!(v)); }
    if let Some(v) = &payload.is_current { update_fields.insert("is_current".into(), json!(v)); }
    if let Some(v) = &payload.location { update_fields.insert("location".into(), json!(v)); }
    update_fields.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let url = format!(
        "{}/rest/v1/profile_experiences?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&json!(update_fields)).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update experience".into()));
    }

    audit::log_action(user_id, "profile.update_experience", "profile_experiences", &id).await;
    ok_message("Experience updated successfully")
}

async fn delete_experience(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let url = format!(
        "{}/rest/v1/profile_experiences?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete experience".into()));
    }
    audit::log_action(user_id, "profile.delete_experience", "profile_experiences", &id).await;
    ok_message("Experience deleted successfully")
}

// ══════════════════════════════════════════════
// Screen 18–19: Education CRUD
// ══════════════════════════════════════════════

async fn add_education(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<EducationInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let edu_id = uuid::Uuid::new_v4().to_string();

    let body = json!({
        "id": edu_id,
        "profile_id": user_id,
        "institution": payload.institution,
        "degree": payload.degree,
        "field_of_study": payload.field_of_study,
        "start_date": payload.start_date,
        "end_date": payload.end_date,
        "description": payload.description.unwrap_or_default(),
        "gpa": payload.gpa,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let url = format!("{}/rest/v1/profile_education", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add education".into()));
    }

    audit::log_action(user_id, "profile.add_education", "profile_education", &edu_id).await;
    ok_json(json!({ "id": edu_id, "message": "Education added successfully" }))
}

async fn update_education(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<EducationInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    let mut update_fields = serde_json::Map::new();
    update_fields.insert("institution".into(), json!(payload.institution));
    update_fields.insert("degree".into(), json!(payload.degree));
    update_fields.insert("field_of_study".into(), json!(payload.field_of_study));
    update_fields.insert("start_date".into(), json!(payload.start_date));
    if let Some(v) = &payload.end_date { update_fields.insert("end_date".into(), json!(v)); }
    if let Some(v) = &payload.description { update_fields.insert("description".into(), json!(v)); }
    if let Some(v) = &payload.gpa { update_fields.insert("gpa".into(), json!(v)); }
    update_fields.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let url = format!(
        "{}/rest/v1/profile_education?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&json!(update_fields)).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update education".into()));
    }

    audit::log_action(user_id, "profile.update_education", "profile_education", &id).await;
    ok_message("Education updated successfully")
}

async fn delete_education(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let url = format!(
        "{}/rest/v1/profile_education?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete education".into()));
    }
    audit::log_action(user_id, "profile.delete_education", "profile_education", &id).await;
    ok_message("Education deleted successfully")
}

// ══════════════════════════════════════════════
// Screen 20: Qualifications
// ══════════════════════════════════════════════

async fn add_qualifications(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<QualificationInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let qual_id = uuid::Uuid::new_v4().to_string();

    let body = json!({
        "id": qual_id,
        "profile_id": user_id,
        "qualification_name": payload.qualification_name,
        "institution": payload.institution,
        "year_obtained": payload.year_obtained,
        "field": payload.field,
        "description": payload.description.unwrap_or_default(),
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let url = format!("{}/rest/v1/profile_qualifications", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add qualification".into()));
    }

    audit::log_action(user_id, "profile.add_qualification", "profile_qualifications", &qual_id).await;
    ok_json(json!({ "id": qual_id, "message": "Qualification added successfully" }))
}

async fn update_qualifications(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<QualificationInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    let mut update_fields = serde_json::Map::new();
    update_fields.insert("qualification_name".into(), json!(payload.qualification_name));
    update_fields.insert("institution".into(), json!(payload.institution));
    if let Some(v) = &payload.year_obtained { update_fields.insert("year_obtained".into(), json!(v)); }
    if let Some(v) = &payload.field { update_fields.insert("field".into(), json!(v)); }
    if let Some(v) = &payload.description { update_fields.insert("description".into(), json!(v)); }
    update_fields.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let url = format!(
        "{}/rest/v1/profile_qualifications?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&json!(update_fields)).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update qualification".into()));
    }
    ok_message("Qualification updated successfully")
}

async fn delete_qualifications(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let url = format!(
        "{}/rest/v1/profile_qualifications?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete qualification".into()));
    }
    ok_message("Qualification deleted successfully")
}

// ══════════════════════════════════════════════
// Screen 21: Licenses
// ══════════════════════════════════════════════

async fn add_license(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<LicenseInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let lic_id = uuid::Uuid::new_v4().to_string();

    let body = json!({
        "id": lic_id,
        "profile_id": user_id,
        "license_name": payload.license_name,
        "issuing_body": payload.issuing_body,
        "license_number": payload.license_number,
        "issued_date": payload.issued_date,
        "expiry_date": payload.expiry_date,
        "verification_status": payload.verification_status.unwrap_or("pending".into()),
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let url = format!("{}/rest/v1/profile_licenses", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add license".into()));
    }

    audit::log_action(user_id, "profile.add_license", "profile_licenses", &lic_id).await;
    ok_json(json!({ "id": lic_id, "message": "License added successfully" }))
}

async fn update_license(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<LicenseInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    let mut update_fields = serde_json::Map::new();
    update_fields.insert("license_name".into(), json!(payload.license_name));
    update_fields.insert("issuing_body".into(), json!(payload.issuing_body));
    update_fields.insert("license_number".into(), json!(payload.license_number));
    if let Some(v) = &payload.issued_date { update_fields.insert("issued_date".into(), json!(v)); }
    if let Some(v) = &payload.expiry_date { update_fields.insert("expiry_date".into(), json!(v)); }
    if let Some(v) = &payload.verification_status { update_fields.insert("verification_status".into(), json!(v)); }
    update_fields.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let url = format!(
        "{}/rest/v1/profile_licenses?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&json!(update_fields)).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update license".into()));
    }
    ok_message("License updated successfully")
}

async fn delete_license(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let url = format!(
        "{}/rest/v1/profile_licenses?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete license".into()));
    }
    ok_message("License deleted successfully")
}

// ══════════════════════════════════════════════
// Screen 22: Skills
// ══════════════════════════════════════════════

async fn add_skills(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<SkillsInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let mut inserted_ids = Vec::new();

    for skill in &payload.skills {
        let skill_id = uuid::Uuid::new_v4().to_string();
        let body = json!({
            "id": skill_id,
            "profile_id": user_id,
            "skill_name": skill.skill_name,
            "category": skill.category.clone().unwrap_or("general".into()),
            "proficiency": skill.proficiency.clone().unwrap_or("intermediate".into()),
            "years_experience": skill.years_experience,
            "created_at": chrono::Utc::now().to_rfc3339(),
        });

        let url = format!("{}/rest/v1/profile_skills", state.rest_url());
        let _ = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await;
        inserted_ids.push(skill_id);
    }

    audit::log_action(user_id, "profile.add_skills", "profile_skills", user_id).await;
    ok_json(json!({ "ids": inserted_ids, "count": inserted_ids.len(), "message": "Skills added successfully" }))
}

async fn delete_skill(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let url = format!(
        "{}/rest/v1/profile_skills?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete skill".into()));
    }
    ok_message("Skill deleted successfully")
}

// ══════════════════════════════════════════════
// Screen 23: Certifications
// ══════════════════════════════════════════════

async fn add_certification(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<CertificationInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let cert_id = uuid::Uuid::new_v4().to_string();

    let body = json!({
        "id": cert_id,
        "profile_id": user_id,
        "certification_name": payload.certification_name,
        "issuing_organization": payload.issuing_organization,
        "issue_date": payload.issue_date,
        "expiry_date": payload.expiry_date,
        "credential_id": payload.credential_id,
        "credential_url": payload.credential_url,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let url = format!("{}/rest/v1/profile_certifications", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add certification".into()));
    }

    audit::log_action(user_id, "profile.add_certification", "profile_certifications", &cert_id).await;
    ok_json(json!({ "id": cert_id, "message": "Certification added successfully" }))
}

async fn update_certification(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<CertificationInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    let mut update_fields = serde_json::Map::new();
    update_fields.insert("certification_name".into(), json!(payload.certification_name));
    update_fields.insert("issuing_organization".into(), json!(payload.issuing_organization));
    if let Some(v) = &payload.issue_date { update_fields.insert("issue_date".into(), json!(v)); }
    if let Some(v) = &payload.expiry_date { update_fields.insert("expiry_date".into(), json!(v)); }
    if let Some(v) = &payload.credential_id { update_fields.insert("credential_id".into(), json!(v)); }
    if let Some(v) = &payload.credential_url { update_fields.insert("credential_url".into(), json!(v)); }
    update_fields.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let url = format!(
        "{}/rest/v1/profile_certifications?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&json!(update_fields)).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update certification".into()));
    }
    ok_message("Certification updated successfully")
}

async fn delete_certification(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let url = format!(
        "{}/rest/v1/profile_certifications?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete certification".into()));
    }
    ok_message("Certification deleted successfully")
}

// ══════════════════════════════════════════════
// Screen 24–25: Research & Publications
// ══════════════════════════════════════════════

async fn add_research(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<ResearchInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let research_id = uuid::Uuid::new_v4().to_string();

    let body = json!({
        "id": research_id,
        "profile_id": user_id,
        "title": payload.title,
        "abstract_text": payload.abstract_text.unwrap_or_default(),
        "publication_date": payload.publication_date,
        "journal": payload.journal,
        "doi": payload.doi,
        "url": payload.url,
        "co_authors": payload.co_authors.unwrap_or_default(),
        "category": payload.category.unwrap_or_default(),
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let url = format!("{}/rest/v1/profile_research", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add research".into()));
    }

    audit::log_action(user_id, "profile.add_research", "profile_research", &research_id).await;
    ok_json(json!({ "id": research_id, "message": "Research added successfully" }))
}

async fn update_research(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<ResearchInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    let mut update_fields = serde_json::Map::new();
    update_fields.insert("title".into(), json!(payload.title));
    if let Some(v) = &payload.abstract_text { update_fields.insert("abstract_text".into(), json!(v)); }
    if let Some(v) = &payload.publication_date { update_fields.insert("publication_date".into(), json!(v)); }
    if let Some(v) = &payload.journal { update_fields.insert("journal".into(), json!(v)); }
    if let Some(v) = &payload.doi { update_fields.insert("doi".into(), json!(v)); }
    if let Some(v) = &payload.url { update_fields.insert("url".into(), json!(v)); }
    if let Some(v) = &payload.co_authors { update_fields.insert("co_authors".into(), json!(v)); }
    if let Some(v) = &payload.category { update_fields.insert("category".into(), json!(v)); }
    update_fields.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let url = format!(
        "{}/rest/v1/profile_research?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&json!(update_fields)).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update research".into()));
    }
    ok_message("Research updated successfully")
}

async fn delete_research(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let url = format!(
        "{}/rest/v1/profile_research?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete research".into()));
    }
    ok_message("Research deleted successfully")
}

async fn add_publication(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<PublicationInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let pub_id = uuid::Uuid::new_v4().to_string();

    let body = json!({
        "id": pub_id,
        "profile_id": user_id,
        "title": payload.title,
        "publication_type": payload.publication_type.unwrap_or_default(),
        "journal_or_publisher": payload.journal_or_publisher.unwrap_or_default(),
        "publication_date": payload.publication_date,
        "doi": payload.doi,
        "url": payload.url,
        "co_authors": payload.co_authors.unwrap_or_default(),
        "abstract_text": payload.abstract_text.unwrap_or_default(),
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let url = format!("{}/rest/v1/profile_publications", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add publication".into()));
    }

    audit::log_action(user_id, "profile.add_publication", "profile_publications", &pub_id).await;
    ok_json(json!({ "id": pub_id, "message": "Publication added successfully" }))
}

async fn update_publication(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
    Json(payload): Json<PublicationInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    let mut update_fields = serde_json::Map::new();
    update_fields.insert("title".into(), json!(payload.title));
    if let Some(v) = &payload.publication_type { update_fields.insert("publication_type".into(), json!(v)); }
    if let Some(v) = &payload.journal_or_publisher { update_fields.insert("journal_or_publisher".into(), json!(v)); }
    if let Some(v) = &payload.publication_date { update_fields.insert("publication_date".into(), json!(v)); }
    if let Some(v) = &payload.doi { update_fields.insert("doi".into(), json!(v)); }
    if let Some(v) = &payload.url { update_fields.insert("url".into(), json!(v)); }
    if let Some(v) = &payload.co_authors { update_fields.insert("co_authors".into(), json!(v)); }
    if let Some(v) = &payload.abstract_text { update_fields.insert("abstract_text".into(), json!(v)); }
    update_fields.insert("updated_at".into(), json!(chrono::Utc::now().to_rfc3339()));

    let url = format!(
        "{}/rest/v1/profile_publications?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.patch(&url).headers(state.supabase_headers()).json(&json!(update_fields)).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to update publication".into()));
    }
    ok_message("Publication updated successfully")
}

async fn delete_publication(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let url = format!(
        "{}/rest/v1/profile_publications?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete publication".into()));
    }
    ok_message("Publication deleted successfully")
}

// ══════════════════════════════════════════════
// Screen 26: Achievements
// ══════════════════════════════════════════════

async fn add_achievement(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(payload): Json<AchievementInput>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let ach_id = uuid::Uuid::new_v4().to_string();

    let body = json!({
        "id": ach_id,
        "profile_id": user_id,
        "title": payload.title,
        "description": payload.description.unwrap_or_default(),
        "date_obtained": payload.date_obtained,
        "category": payload.category.unwrap_or_default(),
        "issuing_organization": payload.issuing_organization,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let url = format!("{}/rest/v1/profile_achievements", state.rest_url());
    let res = state.http.post(&url).headers(state.supabase_headers()).json(&body).send().await?;

    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to add achievement".into()));
    }

    audit::log_action(user_id, "profile.add_achievement", "profile_achievements", &ach_id).await;
    ok_json(json!({ "id": ach_id, "message": "Achievement added successfully" }))
}

async fn delete_achievement(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult {
    let user_id = &auth.claims.sub;
    let url = format!(
        "{}/rest/v1/profile_achievements?id=eq.{}&profile_id=eq.{}",
        state.rest_url(), id, user_id
    );
    let res = state.http.delete(&url).headers(state.supabase_headers()).send().await?;
    if !res.status().is_success() {
        return Err(ApiError::Internal("Failed to delete achievement".into()));
    }
    ok_message("Achievement deleted successfully")
}

// ══════════════════════════════════════════════
// Screen 27: Profile Completion Score
// ══════════════════════════════════════════════

async fn get_completion(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    // Fetch profile core fields
    let url = format!(
        "{}/rest/v1/profiles?id=eq.{}&select=full_name,bio,professional_headline,location,website,avatar_url,cover_photo,country,phone,email_verified",
        state.rest_url(), user_id
    );
    let res = state.http.get(&url).headers(state.supabase_headers()).send().await?;
    let profiles: Value = res.json().await?;

    let profile = profiles
        .as_array()
        .and_then(|a| a.first().cloned())
        .unwrap_or(json!({}));

    // Count completed sections (each worth a portion)
    let mut completed = 0;
    let total_sections = 10;

    // 1. Basic info (name)
    if profile["full_name"].as_str().unwrap_or("").len() > 2 { completed += 1; }
    // 2. Bio
    if profile["bio"].as_str().unwrap_or("").len() > 10 { completed += 1; }
    // 3. Professional headline
    if profile["professional_headline"].as_str().unwrap_or("").len() > 3 { completed += 1; }
    // 4. Location
    if profile["location"].as_str().unwrap_or("").len() > 2 { completed += 1; }
    // 5. Website
    if profile["website"].as_str().unwrap_or("").len() > 5 { completed += 1; }
    // 6. Avatar
    if profile["avatar_url"].as_str().unwrap_or("").len() > 5 { completed += 1; }
    // 7. Cover photo
    if profile["cover_photo"].as_str().unwrap_or("").len() > 5 { completed += 1; }
    // 8. Country
    if profile["country"].as_str().unwrap_or("").len() > 1 { completed += 1; }

    // 9. Has experience
    let exp_url = format!(
        "{}/rest/v1/profile_experiences?profile_id=eq.{}&select=id&limit=1",
        state.rest_url(), user_id
    );
    let exp_res = state.http.get(&exp_url).headers(state.supabase_headers()).send().await?;
    let exp_data: Value = exp_res.json().await.unwrap_or(json!([]));
    if exp_data.as_array().map_or(false, |a| !a.is_empty()) { completed += 1; }

    // 10. Has education
    let edu_url = format!(
        "{}/rest/v1/profile_education?profile_id=eq.{}&select=id&limit=1",
        state.rest_url(), user_id
    );
    let edu_res = state.http.get(&edu_url).headers(state.supabase_headers()).send().await?;
    let edu_data: Value = edu_res.json().await.unwrap_or(json!([]));
    if edu_data.as_array().map_or(false, |a| !a.is_empty()) { completed += 1; }

    let percentage = (completed as f64 / total_sections as f64 * 100.0).round() as i32;

    // Update profile_completed flag
    let update_url = format!("{}/rest/v1/profiles?id=eq.{}", state.rest_url(), user_id);
    let _ = state.http.patch(&update_url).headers(state.supabase_headers())
        .json(&json!({ "profile_completed": percentage >= 80 }))
        .send().await;

    ok_json(json!({
        "completion_percentage": percentage,
        "completed_sections": completed,
        "total_sections": total_sections,
        "sections": {
            "basic_info": profile["full_name"].as_str().unwrap_or("").len() > 2,
            "bio": profile["bio"].as_str().unwrap_or("").len() > 10,
            "professional_headline": profile["professional_headline"].as_str().unwrap_or("").len() > 3,
            "location": profile["location"].as_str().unwrap_or("").len() > 2,
            "website": profile["website"].as_str().unwrap_or("").len() > 5,
            "avatar": profile["avatar_url"].as_str().unwrap_or("").len() > 5,
            "cover_photo": profile["cover_photo"].as_str().unwrap_or("").len() > 5,
            "country": profile["country"].as_str().unwrap_or("").len() > 1,
            "experience": exp_data.as_array().map_or(false, |a| !a.is_empty()),
            "education": edu_data.as_array().map_or(false, |a| !a.is_empty()),
        }
    }))
}

// ══════════════════════════════════════════════
// Screen 28: Profile Analytics
// ══════════════════════════════════════════════

async fn get_analytics(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> ApiResult {
    let user_id = &auth.claims.sub;

    // Count various profile sections
    let mut stats = serde_json::Map::new();

    // Experience count
    let exp_url = format!(
        "{}/rest/v1/profile_experiences?profile_id=eq.{}&select=id",
        state.rest_url(), user_id
    );
    let exp_res = state.http.get(&exp_url).headers(state.supabase_headers()).send().await?;
    let exp_data: Value = exp_res.json().await.unwrap_or(json!([]));
    stats.insert("experience_count".into(), json!(exp_data.as_array().map_or(0, |a| a.len())));

    // Education count
    let edu_url = format!(
        "{}/rest/v1/profile_education?profile_id=eq.{}&select=id",
        state.rest_url(), user_id
    );
    let edu_res = state.http.get(&edu_url).headers(state.supabase_headers()).send().await?;
    let edu_data: Value = edu_res.json().await.unwrap_or(json!([]));
    stats.insert("education_count".into(), json!(edu_data.as_array().map_or(0, |a| a.len())));

    // Skills count
    let skill_url = format!(
        "{}/rest/v1/profile_skills?profile_id=eq.{}&select=id",
        state.rest_url(), user_id
    );
    let skill_res = state.http.get(&skill_url).headers(state.supabase_headers()).send().await?;
    let skill_data: Value = skill_res.json().await.unwrap_or(json!([]));
    stats.insert("skills_count".into(), json!(skill_data.as_array().map_or(0, |a| a.len())));

    // Research count
    let research_url = format!(
        "{}/rest/v1/profile_research?profile_id=eq.{}&select=id",
        state.rest_url(), user_id
    );
    let research_res = state.http.get(&research_url).headers(state.supabase_headers()).send().await?;
    let research_data: Value = research_res.json().await.unwrap_or(json!([]));
    stats.insert("research_count".into(), json!(research_data.as_array().map_or(0, |a| a.len())));

    // Certifications count
    let cert_url = format!(
        "{}/rest/v1/profile_certifications?profile_id=eq.{}&select=id",
        state.rest_url(), user_id
    );
    let cert_res = state.http.get(&cert_url).headers(state.supabase_headers()).send().await?;
    let cert_data: Value = cert_res.json().await.unwrap_or(json!([]));
    stats.insert("certifications_count".into(), json!(cert_data.as_array().map_or(0, |a| a.len())));

    // Licenses count
    let lic_url = format!(
        "{}/rest/v1/profile_licenses?profile_id=eq.{}&select=id",
        state.rest_url(), user_id
    );
    let lic_res = state.http.get(&lic_url).headers(state.supabase_headers()).send().await?;
    let lic_data: Value = lic_res.json().await.unwrap_or(json!([]));
    stats.insert("licenses_count".into(), json!(lic_data.as_array().map_or(0, |a| a.len())));

    // Publications count
    let pub_url = format!(
        "{}/rest/v1/profile_publications?profile_id=eq.{}&select=id",
        state.rest_url(), user_id
    );
    let pub_res = state.http.get(&pub_url).headers(state.supabase_headers()).send().await?;
    let pub_data: Value = pub_res.json().await.unwrap_or(json!([]));
    stats.insert("publications_count".into(), json!(pub_data.as_array().map_or(0, |a| a.len())));

    // Achievements count
    let ach_url = format!(
        "{}/rest/v1/profile_achievements?profile_id=eq.{}&select=id",
        state.rest_url(), user_id
    );
    let ach_res = state.http.get(&ach_url).headers(state.supabase_headers()).send().await?;
    let ach_data: Value = ach_res.json().await.unwrap_or(json!([]));
    stats.insert("achievements_count".into(), json!(ach_data.as_array().map_or(0, |a| a.len())));

    // Qualifications count
    let qual_url = format!(
        "{}/rest/v1/profile_qualifications?profile_id=eq.{}&select=id",
        state.rest_url(), user_id
    );
    let qual_res = state.http.get(&qual_url).headers(state.supabase_headers()).send().await?;
    let qual_data: Value = qual_res.json().await.unwrap_or(json!([]));
    stats.insert("qualifications_count".into(), json!(qual_data.as_array().map_or(0, |a| a.len())));

    // Total profile completeness
    let total_items: i64 = stats.values()
        .filter_map(|v| v.as_i64())
        .sum();
    stats.insert("total_sections_completed".into(), json!(total_items));

    ok_json(json!(stats))
}
