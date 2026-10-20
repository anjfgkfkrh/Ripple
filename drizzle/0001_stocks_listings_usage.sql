CREATE TABLE `listings` (
	`market` text NOT NULL,
	`code` text NOT NULL,
	`name` text NOT NULL,
	`exchange` text NOT NULL,
	`is_etf` integer DEFAULT false NOT NULL,
	PRIMARY KEY(`market`, `code`)
);
--> statement-breakpoint
CREATE INDEX `listings_market_name` ON `listings` (`market`,`name`);--> statement-breakpoint
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
);
--> statement-breakpoint
CREATE UNIQUE INDEX `stocks_market_code` ON `stocks` (`market`,`code`);--> statement-breakpoint
CREATE TABLE `usage_logs` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`at` integer NOT NULL,
	`provider` text NOT NULL,
	`model` text NOT NULL,
	`purpose` text NOT NULL,
	`input_tokens` integer NOT NULL,
	`output_tokens` integer NOT NULL,
	`cost_usd` real NOT NULL
);
--> statement-breakpoint
CREATE INDEX `usage_logs_at` ON `usage_logs` (`at`);