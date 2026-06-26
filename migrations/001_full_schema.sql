-- ============================================================
-- MGN Networking Module — Full Database Schema
-- Run against Supabase SQL Editor or via psql
-- ============================================================

-- ─────────────────────────────────────────────────────────────
-- EXTENSIONS
-- ─────────────────────────────────────────────────────────────
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Helper: auto-update updated_at
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- =============================================================
-- 1. PROFILES  (core identity table)
-- =============================================================
CREATE TABLE IF NOT EXISTS profiles (
    id              UUID PRIMARY KEY,             -- matches auth.users(id) or firebase uid
    auth_id         UUID UNIQUE,
    email           TEXT UNIQUE NOT NULL,
    full_name       TEXT NOT NULL DEFAULT '',
    phone           TEXT,
    phone_verified  BOOLEAN DEFAULT FALSE,
    email_verified  BOOLEAN DEFAULT FALSE,
    profile_verified BOOLEAN DEFAULT FALSE,
    password_hash   TEXT,
    provider        TEXT DEFAULT 'email',
    status          TEXT DEFAULT 'active',      -- active | suspended | deactivated | pending_deletion
    account_status  TEXT DEFAULT 'active',      -- alias for status (some routes use this)
    account_type    TEXT,                        -- professional | organization | admin
    role            TEXT DEFAULT 'user',         -- user | admin | super_admin
    username        TEXT UNIQUE,                 -- handle for @mention
    preferred_name  TEXT,                        -- display name override
    country         TEXT,
    city            TEXT,                        -- city name
    timezone        TEXT DEFAULT 'Asia/Kolkata',
    profile_completed BOOLEAN DEFAULT FALSE,
    completion_score INTEGER DEFAULT 0,
    onboarding_score INTEGER DEFAULT 0,          -- set to 100 after onboarding completes
    badge_color     TEXT DEFAULT 'gray',         -- gray | green | blue | gold
    language        TEXT DEFAULT 'en',
    dialect         TEXT,
    bio             TEXT,
    headline        TEXT,
    location        TEXT,
    company         TEXT,                        -- current organization
    website         TEXT,
    avatar_url      TEXT,
    cover_url       TEXT,
    primary_category TEXT,
    subcategory     TEXT,
    sub_category    TEXT,                        -- alias for subcategory
    interests       JSONB DEFAULT '[]',          -- array of skill/topic strings
    secondary_roles JSONB DEFAULT '[]',          -- array of role strings
    verified        BOOLEAN DEFAULT FALSE,       -- profile verified by admin
    two_fa_enabled  BOOLEAN DEFAULT FALSE,
    two_fa_methods  JSONB DEFAULT '[]',
    last_active     TIMESTAMPTZ DEFAULT NOW(),
    last_password_change TIMESTAMPTZ,
    deactivation_reason TEXT,
    deactivated_at  TIMESTAMPTZ,
    deletion_requested_at TIMESTAMPTZ,
    deletion_scheduled_at TIMESTAMPTZ,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_profiles_email ON profiles(email);
CREATE INDEX IF NOT EXISTS idx_profiles_username ON profiles(username);
CREATE INDEX IF NOT EXISTS idx_profiles_status ON profiles(status);
CREATE INDEX IF NOT EXISTS idx_profiles_account_type ON profiles(account_type);
CREATE INDEX IF NOT EXISTS idx_profiles_category ON profiles(primary_category);
CREATE INDEX IF NOT EXISTS idx_profiles_location ON profiles(location);
CREATE INDEX IF NOT EXISTS idx_profiles_last_active ON profiles(last_active);

-- =============================================================
-- 2. PROFILE SUB-TABLES
-- =============================================================

-- Experiences
CREATE TABLE IF NOT EXISTS profile_experiences (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id  UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    company     TEXT NOT NULL,
    role        TEXT NOT NULL,
    start_date  TEXT,
    end_date    TEXT,
    description TEXT DEFAULT '',
    is_current  BOOLEAN DEFAULT FALSE,
    location    TEXT DEFAULT '',
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_exp_profile ON profile_experiences(profile_id);

-- Education
CREATE TABLE IF NOT EXISTS profile_education (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    institution     TEXT NOT NULL,
    degree          TEXT,
    field_of_study  TEXT,
    start_date      TEXT,
    end_date        TEXT,
    description     TEXT DEFAULT '',
    gpa             TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_edu_profile ON profile_education(profile_id);

-- Qualifications
CREATE TABLE IF NOT EXISTS profile_qualifications (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id          UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    qualification_name  TEXT NOT NULL,
    institution         TEXT,
    year_obtained       INTEGER,
    field               TEXT,
    description         TEXT DEFAULT '',
    created_at          TIMESTAMPTZ DEFAULT NOW(),
    updated_at          TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_qual_profile ON profile_qualifications(profile_id);

-- Licenses
CREATE TABLE IF NOT EXISTS profile_licenses (
    id                    UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id            UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    license_name          TEXT NOT NULL,
    issuing_body          TEXT,
    license_number        TEXT,
    issued_date           TEXT,
    expiry_date           TEXT,
    verification_status   TEXT DEFAULT 'pending', -- pending | verified | rejected
    created_at            TIMESTAMPTZ DEFAULT NOW(),
    updated_at            TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_license_profile ON profile_licenses(profile_id);

-- Skills
CREATE TABLE IF NOT EXISTS profile_skills (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    skill_name      TEXT NOT NULL,
    category        TEXT DEFAULT 'general',
    proficiency     TEXT DEFAULT 'intermediate', -- beginner | intermediate | advanced | expert
    years_experience INTEGER,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_skills_profile ON profile_skills(profile_id);
CREATE INDEX IF NOT EXISTS idx_skills_name ON profile_skills(skill_name);
CREATE INDEX IF NOT EXISTS idx_skills_category ON profile_skills(category);

-- Certifications
CREATE TABLE IF NOT EXISTS profile_certifications (
    id                      UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id              UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    certification_name      TEXT NOT NULL,
    issuing_organization    TEXT,
    issue_date              TEXT,
    expiry_date             TEXT,
    credential_id           TEXT,
    credential_url          TEXT,
    created_at              TIMESTAMPTZ DEFAULT NOW(),
    updated_at              TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_cert_profile ON profile_certifications(profile_id);

-- Research
CREATE TABLE IF NOT EXISTS profile_research (
    id                UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id        UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    title             TEXT NOT NULL,
    abstract_text     TEXT DEFAULT '',
    publication_date  TEXT,
    journal           TEXT,
    doi               TEXT,
    url               TEXT,
    authors           TEXT,
    created_at        TIMESTAMPTZ DEFAULT NOW(),
    updated_at        TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_research_profile ON profile_research(profile_id);

-- Publications
CREATE TABLE IF NOT EXISTS profile_publications (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    authors         TEXT,
    journal_name    TEXT,
    volume          TEXT,
    issue           TEXT,
    pages           TEXT,
    year            INTEGER,
    doi             TEXT,
    url             TEXT,
    abstract        TEXT,
    publication_type TEXT DEFAULT 'article',
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_pub_profile ON profile_publications(profile_id);

-- Achievements
CREATE TABLE IF NOT EXISTS profile_achievements (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    description     TEXT,
    issuer          TEXT,
    date_obtained   TEXT,
    url             TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_achieve_profile ON profile_achievements(profile_id);

-- =============================================================
-- 3. USER SESSIONS
-- =============================================================
CREATE TABLE IF NOT EXISTS user_sessions (
    id              UUID PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    device_name     TEXT DEFAULT 'Unknown',
    browser         TEXT DEFAULT 'Unknown',
    os              TEXT DEFAULT 'Unknown',
    ip_masked       TEXT DEFAULT '0.0.0.xxx',
    country         TEXT,
    city            TEXT,
    is_current      BOOLEAN DEFAULT FALSE,
    two_fa_pending  BOOLEAN DEFAULT FALSE,
    last_active     TIMESTAMPTZ DEFAULT NOW(),
    created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON user_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_current ON user_sessions(is_current);

-- =============================================================
-- 4. SECURITY SETTINGS
-- =============================================================
CREATE TABLE IF NOT EXISTS security_settings (
    id                      UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id                 UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE UNIQUE,
    password_hash           TEXT,
    password_strength       TEXT DEFAULT 'weak',
    two_fa_enabled          BOOLEAN DEFAULT FALSE,
    two_fa_methods          JSONB DEFAULT '[]',
    backup_codes            JSONB DEFAULT '[]',
    backup_codes_generated  BOOLEAN DEFAULT FALSE,
    backup_codes_generated_at TIMESTAMPTZ,
    recovery_email          TEXT,
    recovery_phone          TEXT,
    last_password_change    TIMESTAMPTZ,
    created_at              TIMESTAMPTZ DEFAULT NOW(),
    updated_at              TIMESTAMPTZ DEFAULT NOW()
);

-- =============================================================
-- 5. CONNECTED ACCOUNTS (social logins)
-- =============================================================
CREATE TABLE IF NOT EXISTS connected_accounts (
    id                      UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id                 UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    provider                TEXT NOT NULL, -- email | phone | google | github | linkedin
    provider_account_id     TEXT,
    access_token            TEXT,
    refresh_token           TEXT,
    is_primary              BOOLEAN DEFAULT FALSE,
    connected_at            TIMESTAMPTZ DEFAULT NOW(),
    created_at              TIMESTAMPTZ DEFAULT NOW(),
    updated_at              TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_conn_acct_user ON connected_accounts(user_id);
CREATE INDEX IF NOT EXISTS idx_conn_acct_provider ON connected_accounts(provider);

-- =============================================================
-- 6. OTP VERIFICATIONS
-- =============================================================
CREATE TABLE IF NOT EXISTS otp_verifications (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    code        TEXT NOT NULL,
    purpose     TEXT NOT NULL,  -- email_verify | phone_verify | login_2fa | password_reset
    used        BOOLEAN DEFAULT FALSE,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_otp_user ON otp_verifications(user_id);
CREATE INDEX IF NOT EXISTS idx_otp_code ON otp_verifications(code);

-- =============================================================
-- 7. RELATIONSHIPS
-- =============================================================

-- Connections
CREATE TABLE IF NOT EXISTS connections (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    requester_id    UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    addressee_id    UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    status          TEXT DEFAULT 'pending', -- pending | accepted | declined | removed
    connected_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(requester_id, addressee_id)
);
CREATE INDEX IF NOT EXISTS idx_conn_requester ON connections(requester_id);
CREATE INDEX IF NOT EXISTS idx_conn_addressee ON connections(addressee_id);
CREATE INDEX IF NOT EXISTS idx_conn_status ON connections(status);

-- Follows
CREATE TABLE IF NOT EXISTS follows (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    follower_id     UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    following_id    UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(follower_id, following_id)
);
CREATE INDEX IF NOT EXISTS idx_follows_follower ON follows(follower_id);
CREATE INDEX IF NOT EXISTS idx_follows_following ON follows(following_id);

-- Blocks
CREATE TABLE IF NOT EXISTS blocks (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    blocker_id  UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    blocked_id  UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(blocker_id, blocked_id)
);
CREATE INDEX IF NOT EXISTS idx_blocks_blocker ON blocks(blocker_id);
CREATE INDEX IF NOT EXISTS idx_blocks_blocked ON blocks(blocked_id);

-- Circles
CREATE TABLE IF NOT EXISTS circles (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id    UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT DEFAULT '',
    is_default  BOOLEAN DEFAULT FALSE,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_circles_owner ON circles(owner_id);

-- Circle Members
CREATE TABLE IF NOT EXISTS circle_members (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    circle_id   UUID NOT NULL REFERENCES circles(id) ON DELETE CASCADE,
    member_id   UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(circle_id, member_id)
);
CREATE INDEX IF NOT EXISTS idx_circle_mem_circle ON circle_members(circle_id);

-- =============================================================
-- 8. CONTENT
-- =============================================================

-- Posts
CREATE TABLE IF NOT EXISTS posts (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    author_id       UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    content         TEXT NOT NULL,
    post_type       TEXT DEFAULT 'text',       -- text | image | video | article | poll | event
    visibility      TEXT DEFAULT 'public',      -- public | connections | private
    tags            JSONB DEFAULT '[]',
    media_urls      JSONB DEFAULT '[]',
    likes_count     INTEGER DEFAULT 0,
    comments_count  INTEGER DEFAULT 0,
    shares_count    INTEGER DEFAULT 0,
    bookmarks_count INTEGER DEFAULT 0,
    is_pinned       BOOLEAN DEFAULT FALSE,
    is_edited       BOOLEAN DEFAULT FALSE,
    status          TEXT DEFAULT 'published',   -- published | draft | archived | flagged
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_posts_author ON posts(author_id);
CREATE INDEX IF NOT EXISTS idx_posts_visibility ON posts(visibility);
CREATE INDEX IF NOT EXISTS idx_posts_status ON posts(status);
CREATE INDEX IF NOT EXISTS idx_posts_created ON posts(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_posts_type ON posts(post_type);

-- Post Reactions
CREATE TABLE IF NOT EXISTS post_reactions (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id         UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    reaction_type   TEXT DEFAULT 'like', -- like | love | insightful | celebrate | support
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(post_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_react_post ON post_reactions(post_id);
CREATE INDEX IF NOT EXISTS idx_react_user ON post_reactions(user_id);

-- Post Comments
CREATE TABLE IF NOT EXISTS post_comments (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id             UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id             UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    content             TEXT NOT NULL,
    parent_comment_id   UUID REFERENCES post_comments(id) ON DELETE CASCADE,
    likes_count         INTEGER DEFAULT 0,
    is_edited           BOOLEAN DEFAULT FALSE,
    created_at          TIMESTAMPTZ DEFAULT NOW(),
    updated_at          TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_comment_post ON post_comments(post_id);
CREATE INDEX IF NOT EXISTS idx_comment_user ON post_comments(user_id);
CREATE INDEX IF NOT EXISTS idx_comment_parent ON post_comments(parent_comment_id);

-- Post Bookmarks
CREATE TABLE IF NOT EXISTS post_bookmarks (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id     UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id     UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(post_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_bookmark_user ON post_bookmarks(user_id);

-- =============================================================
-- 9. SEARCH
-- =============================================================

-- Search History
CREATE TABLE IF NOT EXISTS search_history (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    query       TEXT NOT NULL,
    category    TEXT DEFAULT 'all',
    filters     JSONB DEFAULT '{}',
    result_count INTEGER DEFAULT 0,
    created_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_search_user ON search_history(user_id);
CREATE INDEX IF NOT EXISTS idx_search_query ON search_history(query);
CREATE INDEX IF NOT EXISTS idx_search_created ON search_history(created_at DESC);

-- Saved Searches
CREATE TABLE IF NOT EXISTS saved_searches (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    query       TEXT NOT NULL,
    category    TEXT DEFAULT 'all',
    filters     JSONB DEFAULT '{}',
    notify_new  BOOLEAN DEFAULT FALSE,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_saved_search_user ON saved_searches(user_id);

-- =============================================================
-- 10. COMMUNICATION
-- =============================================================

-- Notifications
CREATE TABLE IF NOT EXISTS notifications (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    actor_id        UUID REFERENCES profiles(id),
    notification_type TEXT NOT NULL,  -- connection_request | message | comment | reaction | follow | system
    target_type     TEXT,
    target_id       UUID,
    content         TEXT,
    is_read         BOOLEAN DEFAULT FALSE,
    metadata        JSONB DEFAULT '{}',
    created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_notif_user ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notif_read ON notifications(is_read);
CREATE INDEX IF NOT EXISTS idx_notif_created ON notifications(created_at DESC);

-- Notification Preferences
CREATE TABLE IF NOT EXISTS notification_preferences (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id             UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE UNIQUE,
    channel_in_app      BOOLEAN DEFAULT TRUE,
    channel_email       BOOLEAN DEFAULT TRUE,
    channel_push        BOOLEAN DEFAULT TRUE,
    cat_connections     BOOLEAN DEFAULT TRUE,
    cat_messages        BOOLEAN DEFAULT TRUE,
    cat_research        BOOLEAN DEFAULT TRUE,
    cat_content         BOOLEAN DEFAULT TRUE,
    cat_organization    BOOLEAN DEFAULT TRUE,
    cat_verification    BOOLEAN DEFAULT TRUE,
    cat_security        BOOLEAN DEFAULT TRUE,
    cat_announcements   BOOLEAN DEFAULT TRUE,
    cat_marketing       BOOLEAN DEFAULT FALSE,
    frequency           TEXT DEFAULT 'instant', -- instant | hourly | daily | weekly | never
    quiet_hours_start   TEXT,  -- HH:mm format
    quiet_hours_end     TEXT,  -- HH:mm format
    timezone            TEXT DEFAULT 'Asia/Kolkata',
    created_at          TIMESTAMPTZ DEFAULT NOW(),
    updated_at          TIMESTAMPTZ DEFAULT NOW()
);

-- Conversations
CREATE TABLE IF NOT EXISTS conversations (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    type            TEXT DEFAULT 'direct', -- direct | group
    name            TEXT,
    created_by      UUID NOT NULL REFERENCES profiles(id),
    last_message    TEXT,
    last_message_at TIMESTAMPTZ,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_conv_created ON conversations(created_by);

-- Conversation Members
CREATE TABLE IF NOT EXISTS conversation_members (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    last_read_at    TIMESTAMPTZ,
    joined_at       TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(conversation_id, profile_id)
);
CREATE INDEX IF NOT EXISTS idx_conv_mem_conv ON conversation_members(conversation_id);
CREATE INDEX IF NOT EXISTS idx_conv_mem_profile ON conversation_members(profile_id);

-- Messages
CREATE TABLE IF NOT EXISTS messages (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sender_id       UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    content         TEXT NOT NULL,
    message_type    TEXT DEFAULT 'text', -- text | image | file | system
    reply_to_id     UUID REFERENCES messages(id),
    is_read         BOOLEAN DEFAULT FALSE,
    is_edited       BOOLEAN DEFAULT FALSE,
    edited_at       TIMESTAMPTZ,
    deleted_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_msg_conv ON messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_msg_sender ON messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_msg_created ON messages(created_at DESC);

-- Message Reactions
CREATE TABLE IF NOT EXISTS message_reactions (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    message_id  UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    profile_id  UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    emoji       TEXT NOT NULL,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(message_id, profile_id, emoji)
);
CREATE INDEX IF NOT EXISTS idx_msg_react_msg ON message_reactions(message_id);

-- Typing Indicators
CREATE TABLE IF NOT EXISTS typing_indicators (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    typing_at       TIMESTAMPTZ DEFAULT NOW(),
    expires_at      TIMESTAMPTZ DEFAULT (NOW() + INTERVAL '10 seconds'),
    UNIQUE(conversation_id, profile_id)
);
CREATE INDEX IF NOT EXISTS idx_typing_conv ON typing_indicators(conversation_id);

-- Channels (group chats / broadcast)
CREATE TABLE IF NOT EXISTS channels (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    description     TEXT,
    type            TEXT DEFAULT 'group', -- group | broadcast | announcement
    created_by      UUID NOT NULL REFERENCES profiles(id),
    members_count   INTEGER DEFAULT 0,
    last_message    TEXT,
    last_message_at TIMESTAMPTZ,
    is_archived     BOOLEAN DEFAULT FALSE,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_channel_created ON channels(created_by);

-- Channel Members
CREATE TABLE IF NOT EXISTS channel_members (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    channel_id  UUID NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    profile_id  UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    role        TEXT DEFAULT 'member', -- admin | moderator | member
    last_read_at TIMESTAMPTZ,
    joined_at   TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(channel_id, profile_id)
);
CREATE INDEX IF NOT EXISTS idx_ch_mem_channel ON channel_members(channel_id);

-- Channel Messages
CREATE TABLE IF NOT EXISTS channel_messages (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    channel_id  UUID NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    sender_id   UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    message_type TEXT DEFAULT 'text',
    is_pinned   BOOLEAN DEFAULT FALSE,
    created_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_ch_msg_channel ON channel_messages(channel_id);
CREATE INDEX IF NOT EXISTS idx_ch_msg_created ON channel_messages(created_at DESC);

-- Read Receipts
CREATE TABLE IF NOT EXISTS read_receipts (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    message_id      UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    read_at         TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(message_id, profile_id)
);

-- =============================================================
-- 11. TRUST
-- =============================================================

-- Endorsements
CREATE TABLE IF NOT EXISTS endorsements (
    id                UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    endorser_id       UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    endorsed_id       UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    skill             TEXT NOT NULL,
    comment           TEXT,
    endorsement_type  TEXT DEFAULT 'peer', -- peer | colleague | supervisor | client
    status            TEXT DEFAULT 'active', -- active | withdrawn | flagged
    created_at        TIMESTAMPTZ DEFAULT NOW(),
    updated_at        TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_endorse_endorser ON endorsements(endorser_id);
CREATE INDEX IF NOT EXISTS idx_endorse_endorsed ON endorsements(endorsed_id);
CREATE INDEX IF NOT EXISTS idx_endorse_skill ON endorsements(skill);

-- Verifications
CREATE TABLE IF NOT EXISTS verifications (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    profile_id          UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    verification_type   TEXT NOT NULL, -- identity | education | license | organization | research
    proof_url           TEXT,
    notes               TEXT,
    status              TEXT DEFAULT 'pending', -- pending | verified | rejected
    reviewed_by         UUID REFERENCES profiles(id),
    reviewed_at         TIMESTAMPTZ,
    rejection_reason    TEXT,
    created_at          TIMESTAMPTZ DEFAULT NOW(),
    updated_at          TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_verify_profile ON verifications(profile_id);
CREATE INDEX IF NOT EXISTS idx_verify_status ON verifications(status);

-- =============================================================
-- 12. ORGANIZATIONS
-- =============================================================

CREATE TABLE IF NOT EXISTS organizations (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    description     TEXT,
    org_type        TEXT DEFAULT 'other', -- hospital | clinic | research | pharma | tech | other
    website         TEXT,
    logo_url        TEXT,
    location        TEXT,
    industry        TEXT,
    size            TEXT,
    founded_year    INTEGER,
    is_verified     BOOLEAN DEFAULT FALSE,
    created_by      UUID NOT NULL REFERENCES profiles(id),
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_org_created ON organizations(created_by);
CREATE INDEX IF NOT EXISTS idx_org_type ON organizations(org_type);

-- Organization Members
CREATE TABLE IF NOT EXISTS organization_members (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    role            TEXT DEFAULT 'member', -- owner | admin | member
    department      TEXT,
    title           TEXT,
    joined_at       TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(organization_id, profile_id)
);
CREATE INDEX IF NOT EXISTS idx_org_mem_org ON organization_members(organization_id);
CREATE INDEX IF NOT EXISTS idx_org_mem_profile ON organization_members(profile_id);

-- Organization Invites
CREATE TABLE IF NOT EXISTS organization_invites (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    invited_by      UUID NOT NULL REFERENCES profiles(id),
    email           TEXT NOT NULL,
    role            TEXT DEFAULT 'member',
    department      TEXT,
    message         TEXT,
    token           TEXT UNIQUE NOT NULL,
    status          TEXT DEFAULT 'pending', -- pending | accepted | declined | expired
    expires_at      TIMESTAMPTZ DEFAULT (NOW() + INTERVAL '7 days'),
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_org_invite_org ON organization_invites(organization_id);
CREATE INDEX IF NOT EXISTS idx_org_invite_email ON organization_invites(email);

-- Organization Settings
CREATE TABLE IF NOT EXISTS org_settings (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE UNIQUE,
    settings        JSONB DEFAULT '{}',
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);

-- Teams
CREATE TABLE IF NOT EXISTS teams (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    description     TEXT,
    created_by      UUID NOT NULL REFERENCES profiles(id),
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_team_org ON teams(organization_id);

-- Team Members
CREATE TABLE IF NOT EXISTS team_members (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    team_id     UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    profile_id  UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    role        TEXT DEFAULT 'member', -- lead | member
    joined_at   TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(team_id, profile_id)
);
CREATE INDEX IF NOT EXISTS idx_team_mem_team ON team_members(team_id);

-- =============================================================
-- 13. SETTINGS
-- =============================================================

-- Privacy Settings
CREATE TABLE IF NOT EXISTS privacy_settings (
    id                                      UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id                                 UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE UNIQUE,
    profile_visibility                      TEXT DEFAULT 'public', -- public | verified | connections | only_me
    contact_email_visible                   BOOLEAN DEFAULT FALSE,
    contact_phone_visible                   BOOLEAN DEFAULT FALSE,
    contact_website_visible                 BOOLEAN DEFAULT FALSE,
    contact_social_visible                  BOOLEAN DEFAULT FALSE,
    professional_qualification_visible      BOOLEAN DEFAULT TRUE,
    professional_license_visible            BOOLEAN DEFAULT TRUE,
    professional_experience_visible         BOOLEAN DEFAULT TRUE,
    professional_research_visible           BOOLEAN DEFAULT TRUE,
    professional_organization_visible       BOOLEAN DEFAULT TRUE,
    activity_posts_visible                  BOOLEAN DEFAULT TRUE,
    activity_comments_visible               BOOLEAN DEFAULT TRUE,
    activity_reactions_visible              BOOLEAN DEFAULT TRUE,
    activity_followers_visible              BOOLEAN DEFAULT TRUE,
    activity_following_visible              BOOLEAN DEFAULT TRUE,
    activity_connections_visible            BOOLEAN DEFAULT TRUE,
    messaging_allow_from                    TEXT DEFAULT 'connections', -- everyone | followers | connections | verified | nobody
    search_show_in_search                   BOOLEAN DEFAULT TRUE,
    search_allow_recommendation             BOOLEAN DEFAULT TRUE,
    search_allow_ai_recommendations         BOOLEAN DEFAULT TRUE,
    show_activity_status                    BOOLEAN DEFAULT TRUE,
    allow_tagging                           TEXT DEFAULT 'connections', -- everyone | connections | none
    data_sharing_research                   BOOLEAN DEFAULT FALSE,
    data_sharing_marketing                  BOOLEAN DEFAULT FALSE,
    created_at                              TIMESTAMPTZ DEFAULT NOW(),
    updated_at                              TIMESTAMPTZ DEFAULT NOW()
);

-- =============================================================
-- 14. AUDIT
-- =============================================================

CREATE TABLE IF NOT EXISTS audit_logs (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    actor_id    UUID REFERENCES profiles(id),
    action      TEXT NOT NULL,
    severity    TEXT DEFAULT 'info', -- info | warning | critical
    target_type TEXT,
    target_id   UUID,
    metadata    JSONB DEFAULT '{}',
    ip_hash     TEXT,
    created_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_logs(actor_id);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_logs(action);
CREATE INDEX IF NOT EXISTS idx_audit_target ON audit_logs(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_logs(created_at DESC);

-- =============================================================
-- 15. PERMISSIONS (RBAC)
-- =============================================================

-- Roles
CREATE TABLE IF NOT EXISTS roles (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name        TEXT UNIQUE NOT NULL,
    description TEXT,
    is_system   BOOLEAN DEFAULT FALSE,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW()
);

-- Permissions
CREATE TABLE IF NOT EXISTS permissions (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name        TEXT UNIQUE NOT NULL,
    resource    TEXT NOT NULL,
    action      TEXT NOT NULL, -- create | read | update | delete | manage
    description TEXT,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(resource, action)
);

-- Role Permissions
CREATE TABLE IF NOT EXISTS role_permissions (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    role_id         UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id   UUID NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(role_id, permission_id)
);
CREATE INDEX IF NOT EXISTS idx_role_perm_role ON role_permissions(role_id);

-- User Roles
CREATE TABLE IF NOT EXISTS user_roles (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    role_id     UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    assigned_by UUID REFERENCES profiles(id),
    assigned_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(user_id, role_id)
);
CREATE INDEX IF NOT EXISTS idx_user_role_user ON user_roles(user_id);

-- =============================================================
-- 16. MEDIA
-- =============================================================

CREATE TABLE IF NOT EXISTS media (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    file_name       TEXT NOT NULL,
    file_type       TEXT NOT NULL, -- image | video | document | audio
    mime_type       TEXT,
    file_size       BIGINT,
    storage_path    TEXT NOT NULL,
    public_url      TEXT,
    width           INTEGER,
    height          INTEGER,
    duration_ms     INTEGER,
    thumbnail_url   TEXT,
    alt_text        TEXT,
    tags            JSONB DEFAULT '[]',
    is_public       BOOLEAN DEFAULT TRUE,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_media_user ON media(user_id);
CREATE INDEX IF NOT EXISTS idx_media_type ON media(file_type);

-- Media Albums
CREATE TABLE IF NOT EXISTS media_albums (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    description TEXT,
    is_public   BOOLEAN DEFAULT TRUE,
    cover_url   TEXT,
    items_count INTEGER DEFAULT 0,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_album_user ON media_albums(user_id);

-- Media Album Items
CREATE TABLE IF NOT EXISTS media_album_items (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    album_id    UUID NOT NULL REFERENCES media_albums(id) ON DELETE CASCADE,
    media_id    UUID NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    position    INTEGER DEFAULT 0,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(album_id, media_id)
);

-- =============================================================
-- 17. BACKGROUND JOBS
-- =============================================================

CREATE TABLE IF NOT EXISTS background_jobs (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    job_type        TEXT NOT NULL, -- email | export | report | cleanup | sync
    status          TEXT DEFAULT 'pending', -- pending | running | completed | failed | cancelled
    priority        TEXT DEFAULT 'normal', -- low | normal | high | critical
    payload         JSONB DEFAULT '{}',
    result          JSONB,
    error_message   TEXT,
    created_by      UUID REFERENCES profiles(id),
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    scheduled_at    TIMESTAMPTZ,
    timeout_seconds INTEGER DEFAULT 300,
    max_retries     INTEGER DEFAULT 3,
    retry_count     INTEGER DEFAULT 0,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_job_status ON background_jobs(status);
CREATE INDEX IF NOT EXISTS idx_job_type ON background_jobs(job_type);
CREATE INDEX IF NOT EXISTS idx_job_created ON background_jobs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_job_scheduled ON background_jobs(scheduled_at);

-- =============================================================
-- 18. FEATURE FLAGS
-- =============================================================

CREATE TABLE IF NOT EXISTS feature_flags (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    key                 TEXT UNIQUE NOT NULL,
    name                TEXT NOT NULL,
    description         TEXT,
    enabled             BOOLEAN DEFAULT FALSE,
    rollout_percentage  INTEGER DEFAULT 0, -- 0-100
    target_roles        JSONB DEFAULT '[]',
    target_users        JSONB DEFAULT '[]',
    target_categories   JSONB DEFAULT '[]',
    environment         TEXT DEFAULT 'production', -- production | staging | development
    created_by          UUID REFERENCES profiles(id),
    created_at          TIMESTAMPTZ DEFAULT NOW(),
    updated_at          TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_flag_key ON feature_flags(key);
CREATE INDEX IF NOT EXISTS idx_flag_enabled ON feature_flags(enabled);

-- =============================================================
-- 19. ADMIN
-- =============================================================

-- Admin Audit Logs (separate from regular audit)
CREATE TABLE IF NOT EXISTS admin_audit_logs (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    admin_id        UUID NOT NULL REFERENCES profiles(id),
    action          TEXT NOT NULL,
    target_type     TEXT,
    target_id       UUID,
    details         JSONB DEFAULT '{}',
    ip_hash         TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_admin_audit_admin ON admin_audit_logs(admin_id);
CREATE INDEX IF NOT EXISTS idx_admin_audit_created ON admin_audit_logs(created_at DESC);

-- Moderation Reports
CREATE TABLE IF NOT EXISTS moderation_reports (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    reporter_id     UUID NOT NULL REFERENCES profiles(id),
    target_type     TEXT NOT NULL, -- post | comment | message | profile | channel
    target_id       UUID NOT NULL,
    report_type     TEXT NOT NULL, -- spam | harassment | inappropriate | misinformation | other
    reason          TEXT,
    status          TEXT DEFAULT 'pending', -- pending | reviewing | resolved | dismissed
    reviewed_by     UUID REFERENCES profiles(id),
    action_taken    TEXT,
    action_details  JSONB DEFAULT '{}',
    resolved_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_mod_report_status ON moderation_reports(status);
CREATE INDEX IF NOT EXISTS idx_mod_report_target ON moderation_reports(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_mod_report_created ON moderation_reports(created_at DESC);

-- Platform Config
CREATE TABLE IF NOT EXISTS platform_config (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    key         TEXT UNIQUE NOT NULL,
    value       JSONB NOT NULL DEFAULT '{}',
    updated_by  UUID REFERENCES profiles(id),
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_config_key ON platform_config(key);

-- Announcements
CREATE TABLE IF NOT EXISTS announcements (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    title           TEXT NOT NULL,
    content         TEXT NOT NULL,
    announcement_type TEXT DEFAULT 'info', -- info | warning | critical | maintenance
    target_audience TEXT DEFAULT 'all',    -- all | admin | new | specific
    target_users    JSONB DEFAULT '[]',
    is_active       BOOLEAN DEFAULT TRUE,
    starts_at       TIMESTAMPTZ DEFAULT NOW(),
    expires_at      TIMESTAMPTZ,
    created_by      UUID NOT NULL REFERENCES profiles(id),
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_announce_active ON announcements(is_active);

-- POC Deployments
CREATE TABLE IF NOT EXISTS poc_deployments (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name            TEXT NOT NULL,
    description     TEXT,
    version         TEXT NOT NULL,
    status          TEXT DEFAULT 'staging', -- staging | canary | production | rolled_back
    environment     TEXT DEFAULT 'staging',
    config          JSONB DEFAULT '{}',
    promoted_by     UUID REFERENCES profiles(id),
    promoted_at     TIMESTAMPTZ,
    rolled_back_at  TIMESTAMPTZ,
    created_by      UUID NOT NULL REFERENCES profiles(id),
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);

-- =============================================================
-- 20. ANALYTICS
-- =============================================================

-- Profile Views
CREATE TABLE IF NOT EXISTS profile_views (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    viewer_id       UUID REFERENCES profiles(id),
    viewed_id       UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    source          TEXT DEFAULT 'direct',
    device          TEXT,
    country         TEXT,
    ip_address      INET,
    created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_pv_viewed ON profile_views(viewed_id);
CREATE INDEX IF NOT EXISTS idx_pv_created ON profile_views(created_at DESC);

-- Search Analytics
CREATE TABLE IF NOT EXISTS search_analytics (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    query           TEXT NOT NULL,
    category        TEXT DEFAULT 'all',
    result_count    INTEGER DEFAULT 0,
    user_id         UUID REFERENCES profiles(id),
    created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_sa_query ON search_analytics(query);
CREATE INDEX IF NOT EXISTS idx_sa_created ON search_analytics(created_at DESC);

-- =============================================================
-- 21. RECOMMENDATIONS
-- =============================================================

CREATE TABLE IF NOT EXISTS recommendation_feedback (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id             UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    recommendation_type TEXT NOT NULL, -- people | content | trending
    target_type         TEXT,          -- profile | post | topic
    target_id           UUID,
    action              TEXT NOT NULL, -- click | dismiss | save | connect
    created_at          TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_rec_feed_user ON recommendation_feedback(user_id);
CREATE INDEX IF NOT EXISTS idx_rec_feed_type ON recommendation_feedback(recommendation_type);

-- =============================================================
-- 22. USER EVENTS (real-time)
-- =============================================================

CREATE TABLE IF NOT EXISTS user_events (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    event_type      TEXT NOT NULL,
    target_type     TEXT,
    target_id       UUID,
    payload         JSONB DEFAULT '{}',
    priority        TEXT DEFAULT 'normal', -- low | normal | high | critical
    status          TEXT DEFAULT 'unread', -- unread | read | archived
    read_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_event_user ON user_events(user_id);
CREATE INDEX IF NOT EXISTS idx_event_status ON user_events(status);
CREATE INDEX IF NOT EXISTS idx_event_type ON user_events(event_type);
CREATE INDEX IF NOT EXISTS idx_event_created ON user_events(created_at DESC);

-- User Activity
CREATE TABLE IF NOT EXISTS user_activity (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    activity_type   TEXT NOT NULL, -- login | profile_update | post_created | connection_made | search
    metadata        JSONB DEFAULT '{}',
    ip_hash         TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_activity_user ON user_activity(user_id);
CREATE INDEX IF NOT EXISTS idx_activity_created ON user_activity(created_at DESC);

-- =============================================================
-- 23. CONSENT (GDPR/HIPAA)
-- =============================================================

CREATE TABLE IF NOT EXISTS user_consents (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    consent_type    TEXT NOT NULL, -- analytics | marketing | data_sharing | research | third_party
    purpose         TEXT NOT NULL,
    scope           TEXT,
    status          TEXT DEFAULT 'granted', -- granted | revoked | expired
    granted_at      TIMESTAMPTZ DEFAULT NOW(),
    revoked_at      TIMESTAMPTZ,
    revoke_reason   TEXT,
    ip_hash         TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_consent_user ON user_consents(user_id);
CREATE INDEX IF NOT EXISTS idx_consent_type ON user_consents(consent_type);
CREATE INDEX IF NOT EXISTS idx_consent_status ON user_consents(status);

-- Data Processing Agreements
CREATE TABLE IF NOT EXISTS data_processing_agreements (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    dpa_type    TEXT NOT NULL, -- hipaa | gdpr_standard | research
    agreed      BOOLEAN DEFAULT FALSE,
    signed_at   TIMESTAMPTZ DEFAULT NOW(),
    created_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_dpa_user ON data_processing_agreements(user_id);

-- Data Subject Requests (GDPR)
CREATE TABLE IF NOT EXISTS data_subject_requests (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    request_type    TEXT NOT NULL, -- access | rectification | erasure | portability | restriction
    details         TEXT,
    status          TEXT DEFAULT 'pending', -- pending | processing | completed | rejected
    processed_by    UUID REFERENCES profiles(id),
    processed_at    TIMESTAMPTZ,
    notes           TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    updated_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_dsr_user ON data_subject_requests(user_id);
CREATE INDEX IF NOT EXISTS idx_dsr_status ON data_subject_requests(status);

-- Data Exports
CREATE TABLE IF NOT EXISTS data_exports (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    export_type     TEXT NOT NULL, -- full | profile | posts | connections | analytics | custom
    categories      JSONB,        -- e.g. ["profile","posts"] for custom exports
    status          TEXT DEFAULT 'pending', -- pending | processing | ready | expired | failed
    file_url        TEXT,
    file_size       BIGINT,
    expires_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_export_user ON data_exports(user_id);

-- =============================================================
-- 24. VIEWS (alias for admin compatibility)
-- =============================================================

-- Admin module references "sessions" but actual table is "user_sessions"
CREATE OR REPLACE VIEW sessions AS SELECT * FROM user_sessions;

-- =============================================================
-- 25. LOCALIZATION
-- =============================================================

CREATE TABLE IF NOT EXISTS translations (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    locale      TEXT NOT NULL,
    key         TEXT NOT NULL,
    value       TEXT NOT NULL,
    context     TEXT,
    created_at  TIMESTAMPTZ DEFAULT NOW(),
    updated_at  TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(locale, key)
);
CREATE INDEX IF NOT EXISTS idx_trans_locale ON translations(locale);
CREATE INDEX IF NOT EXISTS idx_trans_key ON translations(key);

-- =============================================================
-- 25. RPC FUNCTIONS
-- =============================================================

-- Check existing conversation between two users
CREATE OR REPLACE FUNCTION check_existing_conversation(user1 UUID, user2 UUID)
RETURNS TABLE(conversation_id UUID, exists_flag BOOLEAN) AS $$
BEGIN
    RETURN QUERY
    SELECT c.id, TRUE
    FROM conversations c
    INNER JOIN conversation_members cm1 ON cm1.conversation_id = c.id AND cm1.profile_id = user1
    INNER JOIN conversation_members cm2 ON cm2.conversation_id = c.id AND cm2.profile_id = user2
    WHERE c.type = 'direct'
    LIMIT 1;
END;
$$ LANGUAGE plpgsql STABLE;

-- =============================================================
-- 26. ROW-LEVEL SECURITY (RLS)
-- Enable RLS on all tables, create policies
-- =============================================================

-- Helper: JWT claim extraction
-- In Supabase, use auth.uid() for the current user's UUID
-- and (auth.jwt() ->> 'role') for role checks

-- Profiles: public read, owner write
ALTER TABLE profiles ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Profiles are viewable by everyone" ON profiles FOR SELECT USING (true);
CREATE POLICY "Users can update own profile" ON profiles FOR UPDATE USING (auth.uid()::text = id::text);
CREATE POLICY "Users can insert own profile" ON profiles FOR INSERT WITH CHECK (auth.uid()::text = id::text);

-- Profile sub-tables: owner read/write
ALTER TABLE profile_experiences ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Experiences visible by everyone" ON profile_experiences FOR SELECT USING (true);
CREATE POLICY "Owner can manage experiences" ON profile_experiences FOR ALL USING (auth.uid()::text = profile_id::text);

ALTER TABLE profile_education ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Education visible by everyone" ON profile_education FOR SELECT USING (true);
CREATE POLICY "Owner can manage education" ON profile_education FOR ALL USING (auth.uid()::text = profile_id::text);

ALTER TABLE profile_qualifications ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Qualifications visible by everyone" ON profile_qualifications FOR SELECT USING (true);
CREATE POLICY "Owner can manage qualifications" ON profile_qualifications FOR ALL USING (auth.uid()::text = profile_id::text);

ALTER TABLE profile_licenses ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Licenses visible by everyone" ON profile_licenses FOR SELECT USING (true);
CREATE POLICY "Owner can manage licenses" ON profile_licenses FOR ALL USING (auth.uid()::text = profile_id::text);

ALTER TABLE profile_skills ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Skills visible by everyone" ON profile_skills FOR SELECT USING (true);
CREATE POLICY "Owner can manage skills" ON profile_skills FOR ALL USING (auth.uid()::text = profile_id::text);

ALTER TABLE profile_certifications ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Certifications visible by everyone" ON profile_certifications FOR SELECT USING (true);
CREATE POLICY "Owner can manage certifications" ON profile_certifications FOR ALL USING (auth.uid()::text = profile_id::text);

ALTER TABLE profile_research ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Research visible by everyone" ON profile_research FOR SELECT USING (true);
CREATE POLICY "Owner can manage research" ON profile_research FOR ALL USING (auth.uid()::text = profile_id::text);

ALTER TABLE profile_publications ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Publications visible by everyone" ON profile_publications FOR SELECT USING (true);
CREATE POLICY "Owner can manage publications" ON profile_publications FOR ALL USING (auth.uid()::text = profile_id::text);

ALTER TABLE profile_achievements ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Achievements visible by everyone" ON profile_achievements FOR SELECT USING (true);
CREATE POLICY "Owner can manage achievements" ON profile_achievements FOR ALL USING (auth.uid()::text = profile_id::text);

-- User Sessions: owner only
ALTER TABLE user_sessions ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own sessions" ON user_sessions FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users manage own sessions" ON user_sessions FOR ALL USING (auth.uid()::text = user_id::text);

-- Security Settings: owner only
ALTER TABLE security_settings ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own security settings" ON security_settings FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users manage own security settings" ON security_settings FOR ALL USING (auth.uid()::text = user_id::text);

-- Connected Accounts: owner only
ALTER TABLE connected_accounts ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own connected accounts" ON connected_accounts FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users manage own connected accounts" ON connected_accounts FOR ALL USING (auth.uid()::text = user_id::text);

-- OTP: owner only
ALTER TABLE otp_verifications ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own OTPs" ON otp_verifications FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "System can insert OTPs" ON otp_verifications FOR INSERT WITH CHECK (true);

-- Connections: participants only
ALTER TABLE connections ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own connections" ON connections FOR SELECT USING (
    auth.uid()::text = requester_id::text OR auth.uid()::text = addressee_id::text
);
CREATE POLICY "Users can create connections" ON connections FOR INSERT WITH CHECK (
    auth.uid()::text = requester_id::text
);
CREATE POLICY "Users can update own connections" ON connections FOR UPDATE USING (
    auth.uid()::text = requester_id::text OR auth.uid()::text = addressee_id::text
);

-- Follows: public read, owner write
ALTER TABLE follows ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Follows are viewable" ON follows FOR SELECT USING (true);
CREATE POLICY "Users can follow" ON follows FOR INSERT WITH CHECK (auth.uid()::text = follower_id::text);
CREATE POLICY "Users can unfollow" ON follows FOR DELETE USING (auth.uid()::text = follower_id::text);

-- Blocks: owner only
ALTER TABLE blocks ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own blocks" ON blocks FOR SELECT USING (auth.uid()::text = blocker_id::text);
CREATE POLICY "Users can block" ON blocks FOR INSERT WITH CHECK (auth.uid()::text = blocker_id::text);
CREATE POLICY "Users can unblock" ON blocks FOR DELETE USING (auth.uid()::text = blocker_id::text);

-- Circles: owner read/write
ALTER TABLE circles ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own circles" ON circles FOR SELECT USING (auth.uid()::text = owner_id::text);
CREATE POLICY "Users manage own circles" ON circles FOR ALL USING (auth.uid()::text = owner_id::text);

-- Circle Members: owner of circle + members
ALTER TABLE circle_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Circle members visible to owner" ON circle_members FOR SELECT USING (
    EXISTS (SELECT 1 FROM circles WHERE circles.id = circle_members.circle_id AND circles.owner_id::text = auth.uid()::text)
);
CREATE POLICY "Circle owner can manage members" ON circle_members FOR ALL USING (
    EXISTS (SELECT 1 FROM circles WHERE circles.id = circle_members.circle_id AND circles.owner_id::text = auth.uid()::text)
);

-- Posts: public read (for public), author write
ALTER TABLE posts ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Public posts are viewable" ON posts FOR SELECT USING (
    visibility = 'public' OR author_id::text = auth.uid()::text
    OR (visibility = 'connections' AND EXISTS (
        SELECT 1 FROM connections WHERE status = 'accepted'
        AND ((connections.requester_id::text = auth.uid()::text AND connections.addressee_id = posts.author_id)
          OR (connections.addressee_id::text = auth.uid()::text AND connections.requester_id = posts.author_id))
    ))
);
CREATE POLICY "Users can create posts" ON posts FOR INSERT WITH CHECK (auth.uid()::text = author_id::text);
CREATE POLICY "Authors can update own posts" ON posts FOR UPDATE USING (auth.uid()::text = author_id::text);
CREATE POLICY "Authors can delete own posts" ON posts FOR DELETE USING (auth.uid()::text = author_id::text);

-- Post Reactions: public read, owner write
ALTER TABLE post_reactions ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Reactions are viewable" ON post_reactions FOR SELECT USING (true);
CREATE POLICY "Users can react" ON post_reactions FOR INSERT WITH CHECK (auth.uid()::text = user_id::text);
CREATE POLICY "Users can unreact" ON post_reactions FOR DELETE USING (auth.uid()::text = user_id::text);

-- Post Comments: public read, author write
ALTER TABLE post_comments ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Comments are viewable" ON post_comments FOR SELECT USING (true);
CREATE POLICY "Users can comment" ON post_comments FOR INSERT WITH CHECK (auth.uid()::text = user_id::text);
CREATE POLICY "Authors can update own comments" ON post_comments FOR UPDATE USING (auth.uid()::text = user_id::text);
CREATE POLICY "Authors can delete own comments" ON post_comments FOR DELETE USING (auth.uid()::text = user_id::text);

-- Post Bookmarks: owner only
ALTER TABLE post_bookmarks ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own bookmarks" ON post_bookmarks FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users can bookmark" ON post_bookmarks FOR INSERT WITH CHECK (auth.uid()::text = user_id::text);
CREATE POLICY "Users can unbookmark" ON post_bookmarks FOR DELETE USING (auth.uid()::text = user_id::text);

-- Search History: owner only
ALTER TABLE search_history ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own search history" ON search_history FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users can record searches" ON search_history FOR INSERT WITH CHECK (auth.uid()::text = user_id::text);
CREATE POLICY "Users can delete own searches" ON search_history FOR DELETE USING (auth.uid()::text = user_id::text);

-- Saved Searches: owner only
ALTER TABLE saved_searches ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own saved searches" ON saved_searches FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users manage own saved searches" ON saved_searches FOR ALL USING (auth.uid()::text = user_id::text);

-- Notifications: owner only
ALTER TABLE notifications ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own notifications" ON notifications FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users can mark own notifications read" ON notifications FOR UPDATE USING (auth.uid()::text = user_id::text);

-- Notification Preferences: owner only
ALTER TABLE notification_preferences ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own notification prefs" ON notification_preferences FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users manage own notification prefs" ON notification_preferences FOR ALL USING (auth.uid()::text = user_id::text);

-- Conversations: members only
ALTER TABLE conversations ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Members see their conversations" ON conversations FOR SELECT USING (
    EXISTS (SELECT 1 FROM conversation_members WHERE conversation_members.conversation_id = conversations.id AND conversation_members.profile_id::text = auth.uid()::text)
);
CREATE POLICY "Users can create conversations" ON conversations FOR INSERT WITH CHECK (auth.uid()::text = created_by::text);

-- Conversation Members: members only
ALTER TABLE conversation_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Members see conversation members" ON conversation_members FOR SELECT USING (
    EXISTS (SELECT 1 FROM conversation_members cm WHERE cm.conversation_id = conversation_members.conversation_id AND cm.profile_id::text = auth.uid()::text)
);

-- Messages: conversation members only
ALTER TABLE messages ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Members see conversation messages" ON messages FOR SELECT USING (
    EXISTS (SELECT 1 FROM conversation_members WHERE conversation_members.conversation_id = messages.conversation_id AND conversation_members.profile_id::text = auth.uid()::text)
);
CREATE POLICY "Members can send messages" ON messages FOR INSERT WITH CHECK (
    auth.uid()::text = sender_id::text
    AND EXISTS (SELECT 1 FROM conversation_members WHERE conversation_members.conversation_id = messages.conversation_id AND conversation_members.profile_id::text = auth.uid()::text)
);

-- Message Reactions: members only
ALTER TABLE message_reactions ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Members see message reactions" ON message_reactions FOR SELECT USING (true);
CREATE POLICY "Users can react to messages" ON message_reactions FOR INSERT WITH CHECK (auth.uid()::text = profile_id::text);

-- Typing Indicators: conversation members
ALTER TABLE typing_indicators ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Members see typing indicators" ON typing_indicators FOR SELECT USING (true);
CREATE POLICY "Users can set typing" ON typing_indicators FOR ALL USING (auth.uid()::text = profile_id::text);

-- Channels: members only
ALTER TABLE channels ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Members see their channels" ON channels FOR SELECT USING (
    EXISTS (SELECT 1 FROM channel_members WHERE channel_members.channel_id = channels.id AND channel_members.profile_id::text = auth.uid()::text)
);
CREATE POLICY "Users can create channels" ON channels FOR INSERT WITH CHECK (auth.uid()::text = created_by::text);

-- Channel Members: channel members only
ALTER TABLE channel_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Members see channel members" ON channel_members FOR SELECT USING (
    EXISTS (SELECT 1 FROM channel_members cm WHERE cm.channel_id = channel_members.channel_id AND cm.profile_id::text = auth.uid()::text)
);

-- Channel Messages: channel members only
ALTER TABLE channel_messages ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Members see channel messages" ON channel_messages FOR SELECT USING (
    EXISTS (SELECT 1 FROM channel_members WHERE channel_members.channel_id = channel_messages.channel_id AND channel_members.profile_id::text = auth.uid()::text)
);
CREATE POLICY "Members can send channel messages" ON channel_messages FOR INSERT WITH CHECK (
    auth.uid()::text = sender_id::text
    AND EXISTS (SELECT 1 FROM channel_members WHERE channel_members.channel_id = channel_messages.channel_id AND channel_members.profile_id::text = auth.uid()::text)
);

-- Read Receipts
ALTER TABLE read_receipts ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own read receipts" ON read_receipts FOR SELECT USING (auth.uid()::text = profile_id::text);
CREATE POLICY "Users can mark read" ON read_receipts FOR INSERT WITH CHECK (auth.uid()::text = profile_id::text);

-- Endorsements: public read, endorser write
ALTER TABLE endorsements ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Endorsements are viewable" ON endorsements FOR SELECT USING (true);
CREATE POLICY "Users can endorse" ON endorsements FOR INSERT WITH CHECK (auth.uid()::text = endorser_id::text);
CREATE POLICY "Endorsers can update own" ON endorsements FOR UPDATE USING (auth.uid()::text = endorser_id::text);

-- Verifications: owner + admins
ALTER TABLE verifications ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own verifications" ON verifications FOR SELECT USING (auth.uid()::text = profile_id::text);
CREATE POLICY "Users can request verification" ON verifications FOR INSERT WITH CHECK (auth.uid()::text = profile_id::text);

-- Organizations: public read, owner/admin write
ALTER TABLE organizations ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Organizations are viewable" ON organizations FOR SELECT USING (true);
CREATE POLICY "Users can create organizations" ON organizations FOR INSERT WITH CHECK (auth.uid()::text = created_by::text);
CREATE POLICY "Creator can update org" ON organizations FOR UPDATE USING (auth.uid()::text = created_by::text);

-- Organization Members: org members
ALTER TABLE organization_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Org members visible" ON organization_members FOR SELECT USING (true);
CREATE POLICY "Org owner/admin can add members" ON organization_members FOR INSERT WITH CHECK (
    EXISTS (SELECT 1 FROM organization_members om WHERE om.organization_id = organization_members.organization_id AND om.profile_id::text = auth.uid()::text AND om.role IN ('owner', 'admin'))
);

-- Organization Invites: org admins
ALTER TABLE organization_invites ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Org invites visible to admins" ON organization_invites FOR SELECT USING (
    EXISTS (SELECT 1 FROM organization_members om WHERE om.organization_id = organization_invites.organization_id AND om.profile_id::text = auth.uid()::text AND om.role IN ('owner', 'admin'))
    OR email = (SELECT email FROM profiles WHERE id::text = auth.uid()::text)
);

-- Org Settings: org members
ALTER TABLE org_settings ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Org settings visible to members" ON org_settings FOR SELECT USING (
    EXISTS (SELECT 1 FROM organization_members om WHERE om.organization_id = org_settings.organization_id AND om.profile_id::text = auth.uid()::text)
);

-- Teams: org members
ALTER TABLE teams ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Teams visible to org members" ON teams FOR SELECT USING (
    EXISTS (SELECT 1 FROM organization_members om WHERE om.organization_id = teams.organization_id AND om.profile_id::text = auth.uid()::text)
);

-- Team Members: team members
ALTER TABLE team_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Team members visible" ON team_members FOR SELECT USING (true);

-- Privacy Settings: owner only
ALTER TABLE privacy_settings ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own privacy settings" ON privacy_settings FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users manage own privacy settings" ON privacy_settings FOR ALL USING (auth.uid()::text = user_id::text);

-- Audit Logs: admins only (read), system (write)
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Admins can view audit logs" ON audit_logs FOR SELECT USING (
    EXISTS (SELECT 1 FROM profiles WHERE id::text = auth.uid()::text AND role = 'admin')
);

-- Roles: public read
ALTER TABLE roles ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Roles are viewable" ON roles FOR SELECT USING (true);

-- Permissions: public read
ALTER TABLE permissions ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Permissions are viewable" ON permissions FOR SELECT USING (true);

-- Role Permissions: public read
ALTER TABLE role_permissions ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Role permissions are viewable" ON role_permissions FOR SELECT USING (true);

-- User Roles: public read
ALTER TABLE user_roles ENABLE ROW LEVEL SECURITY;
CREATE POLICY "User roles are viewable" ON user_roles FOR SELECT USING (true);

-- Media: public read, owner write
ALTER TABLE media ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Public media viewable" ON media FOR SELECT USING (is_public = true OR user_id::text = auth.uid()::text);
CREATE POLICY "Users can upload media" ON media FOR INSERT WITH CHECK (auth.uid()::text = user_id::text);
CREATE POLICY "Users can manage own media" ON media FOR ALL USING (auth.uid()::text = user_id::text);

-- Media Albums: public read, owner write
ALTER TABLE media_albums ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Public albums viewable" ON media_albums FOR SELECT USING (is_public = true OR user_id::text = auth.uid()::text);
CREATE POLICY "Users manage own albums" ON media_albums FOR ALL USING (auth.uid()::text = user_id::text);

-- Media Album Items
ALTER TABLE media_album_items ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Album items viewable" ON media_album_items FOR SELECT USING (true);

-- Background Jobs: creator + system
ALTER TABLE background_jobs ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own jobs" ON background_jobs FOR SELECT USING (
    created_by::text = auth.uid()::text
    OR EXISTS (SELECT 1 FROM profiles WHERE id::text = auth.uid()::text AND role = 'admin')
);

-- Feature Flags: system only (admin)
ALTER TABLE feature_flags ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Admins manage feature flags" ON feature_flags FOR ALL USING (
    EXISTS (SELECT 1 FROM profiles WHERE id::text = auth.uid()::text AND role = 'admin')
);

-- Admin Audit Logs: admins only
ALTER TABLE admin_audit_logs ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Admins can view admin audit logs" ON admin_audit_logs FOR ALL USING (
    EXISTS (SELECT 1 FROM profiles WHERE id::text = auth.uid()::text AND role = 'admin')
);

-- Moderation Reports: reporter + admins
ALTER TABLE moderation_reports ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own reports" ON moderation_reports FOR SELECT USING (
    auth.uid()::text = reporter_id::text
    OR EXISTS (SELECT 1 FROM profiles WHERE id::text = auth.uid()::text AND role = 'admin')
);
CREATE POLICY "Users can create reports" ON moderation_reports FOR INSERT WITH CHECK (auth.uid()::text = reporter_id::text);

-- Platform Config: admins only
ALTER TABLE platform_config ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Admins manage platform config" ON platform_config FOR ALL USING (
    EXISTS (SELECT 1 FROM profiles WHERE id::text = auth.uid()::text AND role = 'admin')
);

-- Announcements: public read, admin write
ALTER TABLE announcements ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Active announcements viewable" ON announcements FOR SELECT USING (is_active = true);
CREATE POLICY "Admins manage announcements" ON announcements FOR ALL USING (
    EXISTS (SELECT 1 FROM profiles WHERE id::text = auth.uid()::text AND role = 'admin')
);

-- POC Deployments: admins only
ALTER TABLE poc_deployments ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Admins manage POC deployments" ON poc_deployments FOR ALL USING (
    EXISTS (SELECT 1 FROM profiles WHERE id::text = auth.uid()::text AND role = 'admin')
);

-- Profile Views: owner + viewer
ALTER TABLE profile_views ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see views of own profile" ON profile_views FOR SELECT USING (auth.uid()::text = viewed_id::text);
CREATE POLICY "Users can record profile views" ON profile_views FOR INSERT WITH CHECK (true);

-- Search Analytics: system
ALTER TABLE search_analytics ENABLE ROW LEVEL SECURITY;
CREATE POLICY "System manages search analytics" ON search_analytics FOR ALL USING (true);

-- Recommendation Feedback: owner only
ALTER TABLE recommendation_feedback ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own recommendation feedback" ON recommendation_feedback FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users can submit feedback" ON recommendation_feedback FOR INSERT WITH CHECK (auth.uid()::text = user_id::text);

-- User Events: owner only
ALTER TABLE user_events ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own events" ON user_events FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "System can create events" ON user_events FOR INSERT WITH CHECK (true);
CREATE POLICY "Users can acknowledge own events" ON user_events FOR UPDATE USING (auth.uid()::text = user_id::text);

-- User Activity: owner only
ALTER TABLE user_activity ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own activity" ON user_activity FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "System can record activity" ON user_activity FOR INSERT WITH CHECK (true);

-- User Consents: owner only
ALTER TABLE user_consents ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own consents" ON user_consents FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users can manage own consents" ON user_consents FOR ALL USING (auth.uid()::text = user_id::text);

-- Data Processing Agreements: owner only
ALTER TABLE data_processing_agreements ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own DPAs" ON data_processing_agreements FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users can sign DPAs" ON data_processing_agreements FOR INSERT WITH CHECK (auth.uid()::text = user_id::text);

-- Data Subject Requests: owner only
ALTER TABLE data_subject_requests ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own data requests" ON data_subject_requests FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users can create data requests" ON data_subject_requests FOR INSERT WITH CHECK (auth.uid()::text = user_id::text);

-- Data Exports: owner only
ALTER TABLE data_exports ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users see own data exports" ON data_exports FOR SELECT USING (auth.uid()::text = user_id::text);
CREATE POLICY "Users can request exports" ON data_exports FOR INSERT WITH CHECK (auth.uid()::text = user_id::text);

-- Translations: public read, admin write
ALTER TABLE translations ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Translations are viewable" ON translations FOR SELECT USING (true);
CREATE POLICY "Admins manage translations" ON translations FOR ALL USING (
    EXISTS (SELECT 1 FROM profiles WHERE id::text = auth.uid()::text AND role = 'admin')
);

-- =============================================================
-- SEED DATA
-- =============================================================

-- Default roles
INSERT INTO roles (id, name, description, is_system) VALUES
    ('a0000000-0000-0000-0000-000000000001', 'user', 'Default user role', true),
    ('a0000000-0000-0000-0000-000000000002', 'admin', 'Platform administrator', true),
    ('a0000000-0000-0000-0000-000000000003', 'moderator', 'Content moderator', true)
ON CONFLICT (name) DO NOTHING;

-- Default permissions
INSERT INTO permissions (id, name, resource, action) VALUES
    ('b0000000-0000-0000-0000-000000000001', 'profiles.read', 'profiles', 'read'),
    ('b0000000-0000-0000-0000-000000000002', 'profiles.update_own', 'profiles', 'update'),
    ('b0000000-0000-0000-0000-000000000003', 'posts.create', 'posts', 'create'),
    ('b0000000-0000-0000-0000-000000000004', 'posts.read', 'posts', 'read'),
    ('b0000000-0000-0000-0000-000000000005', 'posts.delete_own', 'posts', 'delete'),
    ('b0000000-0000-0000-0000-000000000006', 'messages.create', 'messages', 'create'),
    ('b0000000-0000-0000-0000-000000000007', 'messages.read', 'messages', 'read'),
    ('b0000000-0000-0000-0000-000000000008', 'admin.manage_users', 'users', 'manage'),
    ('b0000000-0000-0000-0000-000000000009', 'admin.manage_content', 'content', 'manage'),
    ('b0000000-0000-0000-0000-000000000010', 'admin.view_audit', 'audit', 'read'),
    ('b0000000-0000-0000-0000-000000000011', 'admin.manage_flags', 'feature_flags', 'manage'),
    ('b0000000-0000-0000-0000-000000000012', 'org.create', 'organizations', 'create')
ON CONFLICT (name) DO NOTHING;

-- Assign admin permissions
INSERT INTO role_permissions (role_id, permission_id)
SELECT 'a0000000-0000-0000-0000-000000000002', id FROM permissions
ON CONFLICT DO NOTHING;

-- Default platform config
INSERT INTO platform_config (key, value) VALUES
    ('privacy_policy', '{"version": "1.0", "title": "MGN Privacy Policy", "content": "Your privacy is important to us. This policy describes how we collect, use, and protect your personal information.", "effective_date": "2026-01-01"}'),
    ('terms_of_service', '{"version": "1.0", "title": "MGN Terms of Service", "content": "By using MGN, you agree to these terms.", "effective_date": "2026-01-01"}'),
    ('platform_name', '{"value": "MGN Networking"}'),
    ('maintenance_mode', '{"enabled": false, "message": "System maintenance in progress"}'),
    ('signup_enabled', '{"enabled": true}')
ON CONFLICT (key) DO NOTHING;

-- Seed English translations for key UI strings
INSERT INTO translations (locale, key, value) VALUES
    ('en', 'common.save', 'Save'),
    ('en', 'common.cancel', 'Cancel'),
    ('en', 'common.delete', 'Delete'),
    ('en', 'common.edit', 'Edit'),
    ('en', 'common.search', 'Search'),
    ('en', 'common.loading', 'Loading...'),
    ('en', 'common.error', 'An error occurred'),
    ('en', 'common.success', 'Success'),
    ('en', 'common.confirm', 'Confirm'),
    ('en', 'nav.home', 'Home'),
    ('en', 'nav.profile', 'Profile'),
    ('en', 'nav.network', 'Network'),
    ('en', 'nav.messages', 'Messages'),
    ('en', 'nav.notifications', 'Notifications'),
    ('en', 'nav.settings', 'Settings'),
    ('en', 'auth.login', 'Log In'),
    ('en', 'auth.signup', 'Sign Up'),
    ('en', 'auth.logout', 'Log Out'),
    ('en', 'auth.forgot_password', 'Forgot Password?'),
    ('en', 'profile.about', 'About'),
    ('en', 'profile.experience', 'Experience'),
    ('en', 'profile.education', 'Education'),
    ('en', 'profile.skills', 'Skills'),
    ('en', 'profile.connections', 'Connections'),
    ('en', 'profile.followers', 'Followers'),
    ('en', 'connection.request_sent', 'Connection request sent'),
    ('en', 'connection.accept', 'Accept'),
    ('en', 'connection.decline', 'Decline'),
    ('en', 'post.create', 'Create Post'),
    ('en', 'post.react', 'React'),
    ('en', 'post.comment', 'Comment'),
    ('en', 'post.share', 'Share'),
    ('en', 'message.send', 'Send Message'),
    ('en', 'message.type_placeholder', 'Type a message...'),
    ('en', 'settings.privacy', 'Privacy Settings'),
    ('en', 'settings.security', 'Security Settings'),
    ('en', 'settings.notifications', 'Notification Settings'),
    ('en', 'settings.language', 'Language Settings')
ON CONFLICT (locale, key) DO NOTHING;

-- Seed Hindi translations
INSERT INTO translations (locale, key, value) VALUES
    ('hi', 'common.save', 'सहेजें'),
    ('hi', 'common.cancel', 'रद्द करें'),
    ('hi', 'common.delete', 'हटाएं'),
    ('hi', 'common.edit', 'संपादित करें'),
    ('hi', 'common.search', 'खोजें'),
    ('hi', 'common.loading', 'लोड हो रहा है...'),
    ('hi', 'common.error', 'एक त्रुटि हुई'),
    ('hi', 'common.success', 'सफल'),
    ('hi', 'auth.login', 'लॉग इन'),
    ('hi', 'auth.signup', 'साइन अप'),
    ('hi', 'auth.logout', 'लॉग आउट'),
    ('hi', 'nav.home', 'होम'),
    ('hi', 'nav.profile', 'प्रोफ़ाइल'),
    ('hi', 'nav.network', 'नेटवर्क'),
    ('hi', 'nav.messages', 'संदेश'),
    ('hi', 'nav.notifications', 'सूचनाएं'),
    ('hi', 'nav.settings', 'सेटिंग्स')
ON CONFLICT (locale, key) DO NOTHING;

-- =============================================================
-- DONE
-- =============================================================
