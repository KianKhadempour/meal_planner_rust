CREATE TABLE IF NOT EXISTS `tags`(
    `id`    INT UNSIGNED NOT NULL PRIMARY KEY,
    `likes` INT NOT NULL
);
CREATE TABLE IF NOT EXISTS `recipes`(
    `id`   INT UNSIGNED NOT NULL PRIMARY KEY,
    `name` VARCHAR(255) NOT NULL
);
CREATE TABLE IF NOT EXISTS `previous_recipes`(
    `recipe_id`              INT UNSIGNED NOT NULL,
    FOREIGN KEY(`recipe_id`) REFERENCES recipes(`id`)
);
CREATE TABLE IF NOT EXISTS `recipe_tags`(
    `recipe_id`              INT UNSIGNED NOT NULL,
    `tag_id`                 INT UNSIGNED NOT NULL,
    FOREIGN KEY(`recipe_id`) REFERENCES recipes(`id`),
    FOREIGN KEY(`tag_id`)    REFERENCES tags(`id`)
);
CREATE TABLE IF NOT EXISTS `data`(
    `mode`   INT UNSIGNED NOT NULL DEFAULT 0,
    `offset` INT UNSIGNED NOT NULL DEFAULT 0
);
