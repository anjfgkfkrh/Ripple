CREATE TABLE `feed_headlines` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`title_key` text NOT NULL,
	`title` text NOT NULL,
	`publisher` text NOT NULL,
	`feed` text NOT NULL,
	`published_at` integer NOT NULL,
	`seen_at` integer NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `feed_headlines_title_key` ON `feed_headlines` (`title_key`);--> statement-breakpoint
CREATE INDEX `feed_headlines_published` ON `feed_headlines` (`published_at`);--> statement-breakpoint
CREATE TABLE `market_issues` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`created_at` integer NOT NULL,
	`headline_count` integer NOT NULL,
	`issues` text NOT NULL
);
--> statement-breakpoint
CREATE INDEX `market_issues_created` ON `market_issues` (`created_at`);--> statement-breakpoint
ALTER TABLE `keywords` ADD `kind` text DEFAULT 'structure' NOT NULL;