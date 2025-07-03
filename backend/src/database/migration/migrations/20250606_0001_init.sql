CREATE TABLE IF NOT EXISTS "managers" (
	"_id" INTEGER NOT NULL UNIQUE,
	"id" INTEGER NOT NULL,
	"revision" INTEGER NOT NULL,
	"password" TEXT NOT NULL,
	"totp_secret" BLOB,
	-- only for token revoke
	"token_id" INTEGER NOT NULL,
	-- capacity of unused files in bytes, null for global default, root is unlimited
	"file_capacity" INTEGER,
	PRIMARY KEY("_id")
);

CREATE TABLE IF NOT EXISTS "users" (
	"_id" INTEGER NOT NULL UNIQUE,
	"id" INTEGER NOT NULL,
	"revision" INTEGER NOT NULL,
	"is_deleted" BOOLEAN NOT NULL,
	"name" TEXT NOT NULL,
	-- only for token revoke
	"token_id" INTEGER NOT NULL,
	"wechat_openid" TEXT,
	-- capacity of unused files in bytes, null for global default
	"file_capacity" INTEGER,
	PRIMARY KEY("_id")
);

CREATE TABLE IF NOT EXISTS "projects" (
	"id" INTEGER NOT NULL UNIQUE,
	"name" TEXT NOT NULL,
	"entry_question" TEXT NOT NULL,
	"entry_answer" TEXT NOT NULL,
	"pre_submit_skip_password" TEXT NOT NULL,
	"require_harmony_group_intention" BOOLEAN NOT NULL,
	"non_disclosure_agreement" TEXT,
	"attachment_key" TEXT,
	-- in bytes
	"pre_submit_file_size_min" INTEGER NOT NULL,
	-- in bytes
	"pre_submit_file_size_max" INTEGER NOT NULL,
	-- in bytes
	"submit_file_size_min" INTEGER NOT NULL,
	-- in bytes
	"submit_file_size_max" INTEGER NOT NULL,
	-- in bytes
	"master_file_size_max" INTEGER NOT NULL,
	-- any changes related to this project is not allowed after this time (use rfc3339)
	"end_time" TEXT NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "project_managers" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_id" INTEGER NOT NULL,
	"manager_id" INTEGER NOT NULL,
	"is_revoke" BOOLEAN NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "project_users" (
	"id" INTEGER NOT NULL UNIQUE,
	"user_id" INTEGER NOT NULL,
	"project_id" INTEGER NOT NULL,
	"name" TEXT NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "nda_agreed" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_user_id" INTEGER NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "files" (
	"id" INTEGER NOT NULL UNIQUE,
	-- null if manager_id is set
	"user_id" INTEGER,
	-- null if user_id is set
	"manager_id" INTEGER,
	"name" TEXT NOT NULL,
	-- /uploads/{project}/{user}/{stage}/{md5}
	"s3_key" TEXT NOT NULL,
	-- in bytes
	"size" INTEGER NOT NULL,
	"md5" BLOB NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "uploading_files" (
	"id" INTEGER NOT NULL UNIQUE,
	"file_id" INTEGER NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "pending_files" (
	"id" INTEGER NOT NULL UNIQUE,
	"file_id" INTEGER NOT NULL,
	PRIMARY KEY("id")
);

/* not guarantee the file is finished uploading */
CREATE TABLE IF NOT EXISTS "deleted_files" (
	"id" INTEGER NOT NULL UNIQUE,
	"file_id" INTEGER NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "file_infos" (
	"id" INTEGER NOT NULL,
	"file_id" INTEGER NOT NULL,
	"target_type" TEXT NOT NULL,
	"target_id" INTEGER NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "status_pre_submits" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_user_id" INTEGER NOT NULL,
	"created_at" TEXT NOT NULL,
	"harmony_group_intention" BOOLEAN,
	"comment" TEXT NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "status_submits" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_user_id" INTEGER NOT NULL,
	"created_at" TEXT NOT NULL,
	"comment" TEXT NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "status_masters" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_user_id" INTEGER NOT NULL,
	"manager_id" INTEGER NOT NULL,
	"created_at" TEXT NOT NULL,
	"comment" TEXT NOT NULL,
	PRIMARY KEY("id")
);

/* no file upload for this status */
CREATE TABLE IF NOT EXISTS "status_mixed" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_user_id" INTEGER NOT NULL,
	"manager_id" INTEGER NOT NULL,
	"created_at" TEXT NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "review_pre_submits" (
	"id" INTEGER NOT NULL UNIQUE,
	"manager_id" INTEGER NOT NULL,
	"status" TEXT NOT NULL,
	"lead" BOOLEAN,
	"choir" BOOLEAN,
	"harmony" BOOLEAN,
	"reason" TEXT,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "review_submits" (
	"id" INTEGER NOT NULL UNIQUE,
	"manager_id" INTEGER NOT NULL,
	"status" TEXT NOT NULL,
	"reason" TEXT,
	"reason_detail" TEXT,
	"checked_file_group_id" INTEGER,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "checked_files" (
	"id" INTEGER NOT NULL UNIQUE,
	"group_id" INTEGER NOT NULL,
	"file_id" INTEGER NOT NULL,
	PRIMARY KEY("id")
);

