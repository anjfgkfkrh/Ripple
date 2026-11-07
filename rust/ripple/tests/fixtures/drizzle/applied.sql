PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE IF NOT EXISTS "__drizzle_migrations" (
				id SERIAL PRIMARY KEY,
				hash text NOT NULL,
				created_at numeric
			);
INSERT INTO __drizzle_migrations VALUES(NULL,'cac7212092769a9a6a8cc13d4e51757b0ba2ea59ca3eb6d52a6ef389a58e396a',1792408221227);
INSERT INTO __drizzle_migrations VALUES(NULL,'1615e5bd3abb2f5ced040c575f3b84644db644c865035caad18772ae57da858d',1792461401971);
INSERT INTO __drizzle_migrations VALUES(NULL,'f8a00a0411daf48ba38d587eea710f7c0aa0fcca72aaf97f5aa3323d7214ee36',1792463846231);
INSERT INTO __drizzle_migrations VALUES(NULL,'6aadb3d422cc97c3da9c5e539f4a5c8cb7e613197ac244b70068827e4743bb3b',1792490638588);
INSERT INTO __drizzle_migrations VALUES(NULL,'f1f43e6cb0b4d96598ff18502f7e4456763b21058b398c1a597f779434cdf2d9',1792492794466);
INSERT INTO __drizzle_migrations VALUES(NULL,'c2005dbb88a888b3217825c2eec3cda332335651b1b31f386b2f7acebc2d2322',1792560119923);
INSERT INTO __drizzle_migrations VALUES(NULL,'4dae1f9fb61b35942caf34ee66d0a72518a7c81fe6d48bf03365fac7f497b047',1792635066305);
INSERT INTO __drizzle_migrations VALUES(NULL,'594c0c901151d88c02362542a91252e93468a7cb48c270e151b9acec1337601f',1792653270539);
INSERT INTO __drizzle_migrations VALUES(NULL,'ab68c565c86049a04741635325cc8733f7c2875c9fe873c5eadd2ab7e08e0d0a',1792686962037);
INSERT INTO __drizzle_migrations VALUES(NULL,'f4c06a521984ebaac82988db1f194b4f75d782700333905141c3208eb9f3acea',1793177514864);
INSERT INTO __drizzle_migrations VALUES(NULL,'6952bb1e8a64660862f47ee26bbe3d18f79515c8f6db7896f6858424721e4f64',1793341641528);
CREATE TABLE `settings` (
	`key` text PRIMARY KEY NOT NULL,
	`value` text NOT NULL,
	`updated_at` integer NOT NULL
);
CREATE TABLE `listings` (
	`market` text NOT NULL,
	`code` text NOT NULL,
	`name` text NOT NULL,
	`exchange` text NOT NULL,
	`is_etf` integer DEFAULT false NOT NULL,
	PRIMARY KEY(`market`, `code`)
);
CREATE TABLE `stocks` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`market` text NOT NULL,
	`code` text NOT NULL,
	`name` text NOT NULL,
	`name_ko` text,
	`exchange` text NOT NULL,
	`user_input` text NOT NULL,
	`created_at` integer NOT NULL,
	`deleted_at` integer
, `keywords_status` text DEFAULT 'pending' NOT NULL, `keywords_error` text, `keywords_generated_at` integer, `backfill_from` integer, `kind` text DEFAULT 'stock' NOT NULL, `topic_id` integer REFERENCES topics(id));
CREATE TABLE `usage_logs` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`at` integer NOT NULL,
	`provider` text NOT NULL,
	`model` text NOT NULL,
	`purpose` text NOT NULL,
	`input_tokens` integer NOT NULL,
	`output_tokens` integer NOT NULL,
	`cost_usd` real NOT NULL
, `stock_id` integer);
CREATE TABLE `keywords` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`stock_id` integer NOT NULL,
	`level` integer NOT NULL,
	`parent_id` integer,
	`text_ko` text NOT NULL,
	`text_en` text,
	`reason` text NOT NULL,
	`path` text NOT NULL,
	`created_at` integer NOT NULL, `kind` text DEFAULT 'structure' NOT NULL, `relevance` real, `magnitude` real, `topic_id` integer REFERENCES topics(id),
	FOREIGN KEY (`stock_id`) REFERENCES `stocks`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`parent_id`) REFERENCES `keywords`(`id`) ON UPDATE no action ON DELETE cascade
);
CREATE TABLE `article_matches` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`article_id` integer NOT NULL,
	`stock_id` integer NOT NULL,
	`keyword_path` text NOT NULL,
	`match_kind` text NOT NULL,
	`level` integer NOT NULL,
	`created_at` integer NOT NULL, `keyword_reason` text,
	FOREIGN KEY (`article_id`) REFERENCES `articles`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`stock_id`) REFERENCES `stocks`(`id`) ON UPDATE no action ON DELETE no action
);
CREATE TABLE `articles` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`url_key` text NOT NULL,
	`url` text NOT NULL,
	`title` text NOT NULL,
	`summary` text,
	`publisher` text NOT NULL,
	`type` text DEFAULT 'news' NOT NULL,
	`feed` text NOT NULL,
	`published_at` integer NOT NULL,
	`collected_at` integer NOT NULL
);
CREATE TABLE `collection_runs` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`trigger` text NOT NULL,
	`status` text NOT NULL,
	`started_at` integer NOT NULL,
	`finished_at` integer,
	`since` integer NOT NULL,
	`sources` text DEFAULT '[]' NOT NULL,
	`articles_new` integer DEFAULT 0 NOT NULL,
	`matches_new` integer DEFAULT 0 NOT NULL,
	`note` text
, `evaluation` text);
CREATE TABLE `evaluations` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`match_id` integer NOT NULL,
	`article_id` integer NOT NULL,
	`stock_id` integer NOT NULL,
	`relevance` real NOT NULL,
	`relevance_confidence` real NOT NULL,
	`impact` text NOT NULL,
	`impact_confidence` real NOT NULL,
	`impact_probabilities` text NOT NULL,
	`model` text NOT NULL,
	`evaluated_at` integer NOT NULL, `magnitude` real, `magnitude_confidence` real, `copied_from` integer,
	FOREIGN KEY (`match_id`) REFERENCES `article_matches`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`article_id`) REFERENCES `articles`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`stock_id`) REFERENCES `stocks`(`id`) ON UPDATE no action ON DELETE no action
);
CREATE TABLE `feed_headlines` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`title_key` text NOT NULL,
	`title` text NOT NULL,
	`publisher` text NOT NULL,
	`feed` text NOT NULL,
	`published_at` integer NOT NULL,
	`seen_at` integer NOT NULL
, `url` text, `url_key` text, `summary` text);
CREATE TABLE `market_issues` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`created_at` integer NOT NULL,
	`headline_count` integer NOT NULL,
	`issues` text NOT NULL
);
CREATE TABLE `topics` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`source` text NOT NULL,
	`name` text NOT NULL,
	`summary` text,
	`terms` text DEFAULT '[]' NOT NULL,
	`status` text DEFAULT 'active' NOT NULL,
	`created_at` integer NOT NULL,
	`last_seen_at` integer,
	`ended_at` integer,
	`deleted_at` integer
);
CREATE TABLE `article_reads` (
	`article_id` integer PRIMARY KEY NOT NULL,
	`read_at` integer NOT NULL,
	FOREIGN KEY (`article_id`) REFERENCES `articles`(`id`) ON UPDATE no action ON DELETE cascade
);
CREATE TABLE `stock_prices` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`stock_id` integer NOT NULL,
	`price` real NOT NULL,
	`change` real NOT NULL,
	`change_percent` real NOT NULL,
	`source` text NOT NULL,
	`quoted_at` integer,
	`fetched_at` integer NOT NULL,
	FOREIGN KEY (`stock_id`) REFERENCES `stocks`(`id`) ON UPDATE no action ON DELETE no action
);
CREATE INDEX `listings_market_name` ON `listings` (`market`,`name`);
CREATE UNIQUE INDEX `stocks_market_code` ON `stocks` (`market`,`code`);
CREATE INDEX `usage_logs_at` ON `usage_logs` (`at`);
CREATE INDEX `keywords_stock` ON `keywords` (`stock_id`);
CREATE UNIQUE INDEX `article_matches_article_stock` ON `article_matches` (`article_id`,`stock_id`);
CREATE INDEX `article_matches_stock` ON `article_matches` (`stock_id`);
CREATE UNIQUE INDEX `articles_url_key` ON `articles` (`url_key`);
CREATE INDEX `articles_published` ON `articles` (`published_at`);
CREATE INDEX `collection_runs_started` ON `collection_runs` (`started_at`);
CREATE UNIQUE INDEX `evaluations_match` ON `evaluations` (`match_id`);
CREATE INDEX `evaluations_stock_relevance` ON `evaluations` (`stock_id`,`relevance`);
CREATE UNIQUE INDEX `feed_headlines_title_key` ON `feed_headlines` (`title_key`);
CREATE INDEX `feed_headlines_published` ON `feed_headlines` (`published_at`);
CREATE INDEX `market_issues_created` ON `market_issues` (`created_at`);
CREATE INDEX `topics_source_status` ON `topics` (`source`,`status`);
CREATE INDEX `stock_prices_stock_fetched` ON `stock_prices` (`stock_id`,`fetched_at`);
COMMIT;
