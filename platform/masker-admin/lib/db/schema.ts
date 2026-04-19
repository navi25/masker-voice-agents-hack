import {
  pgTable,
  text,
  timestamp,
  integer,
  uuid,
} from "drizzle-orm/pg-core";

// ── NextAuth required tables ──────────────────────────────────────────────────

export const users = pgTable("users", {
  id: text("id").primaryKey(),
  name: text("name"),
  email: text("email").notNull().unique(),
  emailVerified: timestamp("email_verified", { mode: "date" }),
  image: text("image"),
});

export const accounts = pgTable("accounts", {
  userId: text("user_id").notNull().references(() => users.id, { onDelete: "cascade" }),
  type: text("type").notNull(),
  provider: text("provider").notNull(),
  providerAccountId: text("provider_account_id").notNull(),
  refresh_token: text("refresh_token"),
  access_token: text("access_token"),
  expires_at: integer("expires_at"),
  token_type: text("token_type"),
  scope: text("scope"),
  id_token: text("id_token"),
  session_state: text("session_state"),
});

export const sessions = pgTable("sessions", {
  sessionToken: text("session_token").primaryKey(),
  userId: text("user_id").notNull().references(() => users.id, { onDelete: "cascade" }),
  expires: timestamp("expires", { mode: "date" }).notNull(),
});

export const verificationTokens = pgTable("verification_tokens", {
  identifier: text("identifier").notNull(),
  token: text("token").notNull(),
  expires: timestamp("expires", { mode: "date" }).notNull(),
});

// ── Masker tables ─────────────────────────────────────────────────────────────

export const orgs = pgTable("orgs", {
  id: uuid("id").defaultRandom().primaryKey(),
  name: text("name").notNull(),
  slug: text("slug").notNull().unique(),
  ownerId: text("owner_id").notNull().references(() => users.id, { onDelete: "cascade" }),
  framework: text("framework").notNull().default("HIPAA"),
  createdAt: timestamp("created_at", { mode: "date" }).defaultNow().notNull(),
});

export const apiKeys = pgTable("api_keys", {
  id: uuid("id").defaultRandom().primaryKey(),
  orgId: uuid("org_id").notNull().references(() => orgs.id, { onDelete: "cascade" }),
  label: text("label").notNull(),
  keyHash: text("key_hash").notNull(),
  prefix: text("prefix").notNull(),
  permissions: text("permissions").array().notNull().default([]),
  environment: text("environment").notNull().default("production"),
  status: text("status").notNull().default("active"), // active | revoked
  lastUsedAt: timestamp("last_used_at", { mode: "date" }),
  createdAt: timestamp("created_at", { mode: "date" }).defaultNow().notNull(),
});

export const kmsKeys = pgTable("kms_keys", {
  id: uuid("id").defaultRandom().primaryKey(),
  orgId: uuid("org_id").notNull().references(() => orgs.id, { onDelete: "cascade" }),
  alias: text("alias").notNull(),
  scope: text("scope").notNull().default("All sessions"),
  region: text("region").notNull().default("us-east-1"),
  rotationCadence: text("rotation_cadence").notNull().default("90 days"),
  lastRotatedAt: timestamp("last_rotated_at", { mode: "date" }).defaultNow().notNull(),
  status: text("status").notNull().default("active"), // active | rotating | disabled
  provider: text("provider").notNull().default("masker"), // masker | byok
  createdAt: timestamp("created_at", { mode: "date" }).defaultNow().notNull(),
});

export const auditLogs = pgTable("audit_logs", {
  id: uuid("id").defaultRandom().primaryKey(),
  orgId: uuid("org_id").notNull().references(() => orgs.id, { onDelete: "cascade" }),
  sessionId: text("session_id").notNull(),
  channel: text("channel").notNull().default("voice"),
  useCase: text("use_case").notNull().default("general"),
  policyVersion: text("policy_version").notNull().default("1.0"),
  status: text("status").notNull().default("allowed"), // allowed | masked | blocked
  riskLevel: text("risk_level").notNull().default("low"), // low | medium | high | critical
  entitiesDetected: integer("entities_detected").notNull().default(0),
  duration: text("duration").notNull().default("0ms"),
  rawHash: text("raw_hash").notNull().default(""),
  redactedTranscript: text("redacted_transcript").notNull().default(""),
  entitySpans: text("entity_spans").notNull().default("[]"), // JSON string
  createdAt: timestamp("created_at", { mode: "date" }).defaultNow().notNull(),
});

// ── Convenience types ─────────────────────────────────────────────────────────

export type Org = typeof orgs.$inferSelect;
export type ApiKey = typeof apiKeys.$inferSelect;
export type KmsKey = typeof kmsKeys.$inferSelect;
export type AuditLog = typeof auditLogs.$inferSelect;
