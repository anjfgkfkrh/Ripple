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
	`evaluated_at` integer NOT NULL,
	FOREIGN KEY (`match_id`) REFERENCES `article_matches`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`article_id`) REFERENCES `articles`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`stock_id`) REFERENCES `stocks`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `evaluations_match` ON `evaluations` (`match_id`);--> statement-breakpoint
CREATE INDEX `evaluations_stock_relevance` ON `evaluations` (`stock_id`,`relevance`);--> statement-breakpoint
ALTER TABLE `article_matches` ADD `keyword_reason` text;--> statement-breakpoint
ALTER TABLE `collection_runs` ADD `evaluation` text;