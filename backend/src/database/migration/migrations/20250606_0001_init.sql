CREATE TABLE IF NOT EXISTS "users" (
	"_id" INTEGER NOT NULL UNIQUE,
	"id" INTEGER NOT NULL,
	"revision" INTEGER NOT NULL,
	"is_deleted" BOOLEAN NOT NULL,
	"name" TEXT NOT NULL,
	"token_id" INTEGER NOT NULL,
	"wechat_openid" TEXT,
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
	"attachment_file_id" INTEGER,
	"pre_submit_file_size_min" INTEGER NOT NULL,
	"pre_submit_file_size_max" INTEGER NOT NULL,
	"submit_file_size_min" INTEGER NOT NULL,
	"submit_file_size_max" INTEGER NOT NULL,
	"master_file_size_max" INTEGER NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("attachment_file_id") REFERENCES "files"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "project_users" (
	"id" INTEGER NOT NULL UNIQUE,
	"user_id" INTEGER NOT NULL,
	"project_id" INTEGER NOT NULL,
	"name" TEXT NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("user_id") REFERENCES "users"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION,
	FOREIGN KEY ("project_id") REFERENCES "projects"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "files" (
	"id" INTEGER NOT NULL UNIQUE,
	"name" TEXT NOT NULL,
	"sha256sum" BLOB NOT NULL,
	"size" INTEGER NOT NULL,
	PRIMARY KEY("id")
);

CREATE TABLE IF NOT EXISTS "pending_files" (
	"id" INTEGER NOT NULL UNIQUE,
	"file_id" INTEGER NOT NULL,
	"project_user_id" INTEGER NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("file_id") REFERENCES "files"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION,
	FOREIGN KEY ("project_user_id") REFERENCES "project_users"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "deleted_files" (
	"id" INTEGER NOT NULL UNIQUE,
	"file_id" INTEGER NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("file_id") REFERENCES "files"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "file_infos" (
	"id" INTEGER NOT NULL,
	"file_id" INTEGER NOT NULL,
	"target_type" TEXT NOT NULL,
	"target_id" INTEGER NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("file_id") REFERENCES "files"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "pre_submits" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_user_id" INTEGER NOT NULL,
	"created_at" TEXT NOT NULL,
	"harmony_group_intention" BOOLEAN,
	"comment" TEXT NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("project_user_id") REFERENCES "project_users"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "pre_submit_reviews" (
	"id" INTEGER NOT NULL UNIQUE,
	"status" TEXT NOT NULL,
	"lead" BOOLEAN,
	"choir" BOOLEAN,
	"harmony" BOOLEAN,
	"reason" BOOLEAN,
	PRIMARY KEY("id"),
	FOREIGN KEY ("id") REFERENCES "pre_submits"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "submits" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_user_id" INTEGER NOT NULL,
	"created_at" TEXT NOT NULL,
	"comment" TEXT NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("project_user_id") REFERENCES "project_users"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "submit_reviews" (
	"id" INTEGER NOT NULL UNIQUE,
	"status" TEXT NOT NULL,
	"reason" BOOLEAN,
	"reason_detail" TEXT,
	"checked_file_group_id" INTEGER,
	PRIMARY KEY("id"),
	FOREIGN KEY ("id") REFERENCES "submits"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION,
	FOREIGN KEY ("checked_file_group_id") REFERENCES "checked_files"("group_id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "masters" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_user_id" INTEGER NOT NULL,
	"created_at" TEXT NOT NULL,
	"comment" TEXT NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("project_user_id") REFERENCES "project_users"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "checked_files" (
	"id" INTEGER NOT NULL UNIQUE,
	"group_id" INTEGER NOT NULL,
	"file_id" INTEGER NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("file_id") REFERENCES "file_infos"("file_id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "managers" (
	"_id" INTEGER NOT NULL UNIQUE,
	"id" INTEGER NOT NULL,
	"revision" INTEGER NOT NULL,
	"pswd_argon2" BLOB NOT NULL,
	"argon2_m" INTEGER NOT NULL,
	"argon2_t" INTEGER NOT NULL,
	"argon2_p" INTEGER NOT NULL,
	"argon2_len" INTEGER NOT NULL,
	"pswd_salt" BLOB NOT NULL,
	"totp_secret" BLOB,
	"token_id" INTEGER NOT NULL,
	PRIMARY KEY("_id")
);

CREATE TABLE IF NOT EXISTS "pending_attachments" (
	"id" INTEGER NOT NULL UNIQUE,
	"file_id" INTEGER NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("file_id") REFERENCES "files"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);

CREATE TABLE IF NOT EXISTS "project_managers" (
	"id" INTEGER NOT NULL UNIQUE,
	"project_id" INTEGER NOT NULL,
	"manager_id" INTEGER NOT NULL,
	"is_revoke" BOOLEAN NOT NULL,
	PRIMARY KEY("id"),
	FOREIGN KEY ("manager_id") REFERENCES "managers"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION,
	FOREIGN KEY ("project_id") REFERENCES "projects"("id")
	ON UPDATE NO ACTION ON DELETE NO ACTION
);
