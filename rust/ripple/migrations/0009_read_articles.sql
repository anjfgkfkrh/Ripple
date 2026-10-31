CREATE TABLE `article_reads` (
	`article_id` integer PRIMARY KEY NOT NULL,
	`read_at` integer NOT NULL,
	FOREIGN KEY (`article_id`) REFERENCES `articles`(`id`) ON UPDATE no action ON DELETE cascade
);
