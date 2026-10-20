CREATE TABLE `keywords` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`stock_id` integer NOT NULL,
	`level` integer NOT NULL,
	`parent_id` integer,
	`text_ko` text NOT NULL,
	`text_en` text,
	`reason` text NOT NULL,
	`path` text NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`stock_id`) REFERENCES `stocks`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`parent_id`) REFERENCES `keywords`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `keywords_stock` ON `keywords` (`stock_id`);--> statement-breakpoint
ALTER TABLE `stocks` ADD `keywords_status` text DEFAULT 'pending' NOT NULL;--> statement-breakpoint
ALTER TABLE `stocks` ADD `keywords_error` text;--> statement-breakpoint
ALTER TABLE `stocks` ADD `keywords_generated_at` integer;