CREATE TABLE `article_matches` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`article_id` integer NOT NULL,
	`stock_id` integer NOT NULL,
	`keyword_path` text NOT NULL,
	`match_kind` text NOT NULL,
	`level` integer NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`article_id`) REFERENCES `articles`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`stock_id`) REFERENCES `stocks`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `article_matches_article_stock` ON `article_matches` (`article_id`,`stock_id`);--> statement-breakpoint
CREATE INDEX `article_matches_stock` ON `article_matches` (`stock_id`);--> statement-breakpoint
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
--> statement-breakpoint
CREATE UNIQUE INDEX `articles_url_key` ON `articles` (`url_key`);--> statement-breakpoint
CREATE INDEX `articles_published` ON `articles` (`published_at`);--> statement-breakpoint
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
);
--> statement-breakpoint
CREATE INDEX `collection_runs_started` ON `collection_runs` (`started_at`);