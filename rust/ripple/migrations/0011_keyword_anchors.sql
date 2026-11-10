ALTER TABLE `stocks` ADD `aliases` text DEFAULT '[]' NOT NULL;--> statement-breakpoint
ALTER TABLE `stocks` ADD `anchors` text DEFAULT '[]' NOT NULL;--> statement-breakpoint
ALTER TABLE `stocks` ADD `aliases_removed` text DEFAULT '[]' NOT NULL;--> statement-breakpoint
ALTER TABLE `topics` ADD `scope` text DEFAULT 'market' NOT NULL;--> statement-breakpoint
ALTER TABLE `article_matches` ADD `gated` integer DEFAULT 0 NOT NULL;--> statement-breakpoint
UPDATE `stocks` SET `keywords_status` = 'pending', `keywords_error` = NULL WHERE `kind` = 'stock' AND `deleted_at` IS NULL;
