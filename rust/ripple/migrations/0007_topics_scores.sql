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
--> statement-breakpoint
CREATE INDEX `topics_source_status` ON `topics` (`source`,`status`);--> statement-breakpoint
ALTER TABLE `evaluations` ADD `magnitude` real;--> statement-breakpoint
ALTER TABLE `evaluations` ADD `magnitude_confidence` real;--> statement-breakpoint
ALTER TABLE `evaluations` ADD `copied_from` integer;--> statement-breakpoint
ALTER TABLE `feed_headlines` ADD `url` text;--> statement-breakpoint
ALTER TABLE `feed_headlines` ADD `url_key` text;--> statement-breakpoint
ALTER TABLE `feed_headlines` ADD `summary` text;--> statement-breakpoint
ALTER TABLE `keywords` ADD `relevance` real;--> statement-breakpoint
ALTER TABLE `keywords` ADD `magnitude` real;--> statement-breakpoint
ALTER TABLE `keywords` ADD `topic_id` integer REFERENCES topics(id);--> statement-breakpoint
ALTER TABLE `stocks` ADD `kind` text DEFAULT 'stock' NOT NULL;--> statement-breakpoint
ALTER TABLE `stocks` ADD `topic_id` integer REFERENCES topics(id);--> statement-breakpoint
ALTER TABLE `usage_logs` ADD `stock_id` integer;