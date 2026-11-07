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
--> statement-breakpoint
CREATE INDEX `stock_prices_stock_fetched` ON `stock_prices` (`stock_id`,`fetched_at`);