-- 피드가 준 미래 발행 시각(국민일보 지면 기사, 2026-10-23 결정): 받은 시각보다 10분 넘게 미래면 받은 시각으로 고친다.
UPDATE `articles` SET `published_at` = `collected_at` WHERE `published_at` > `collected_at` + 600000;
--> statement-breakpoint
UPDATE `feed_headlines` SET `published_at` = `seen_at` WHERE `published_at` > `seen_at` + 600000;
