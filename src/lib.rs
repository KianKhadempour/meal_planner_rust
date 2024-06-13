pub mod api {
    pub use models::Recipe;
    use models::{Component, IncompatibleComponentError, RecipeList};
    use reqwest::header::{ACCEPT, ACCEPT_ENCODING, HOST, USER_AGENT};

    const BASE_URL: &str = "https://tasty.p.rapidapi.com";

    pub async fn get_recipes_list(
        offset: i64,
        size: i64,
        rapidapi_key: &str,
    ) -> Result<Vec<Recipe>, reqwest::Error> {
        let client = reqwest::Client::new();
        let response = client
            .get(BASE_URL.to_owned() + "/recipes/list")
            .header("X-RAPIDAPI-KEY", rapidapi_key)
            .header("X-RAPIDAPI-HOST", "tasty.p.rapidapi.com")
            .header(USER_AGENT, "rust reqwest client")
            .header(ACCEPT, "*/*")
            .header(ACCEPT_ENCODING, "gzip, deflate")
            .header(HOST, "tasty.p.rapidapi.com")
            .query(&[("from", offset), ("size", size)])
            .send()
            .await?
            .json::<RecipeList>()
            .await;

        match response {
            Ok(recipe_list) => Ok(recipe_list.results),
            Err(e) => {
                eprintln!("Failed to parse the API response!");
                Err(e)
            }
        }
    }

    pub fn get_components(recipes: &Vec<Recipe>) -> Vec<Component> {
        let mut ret: Vec<Component> = Vec::new();

        for recipe in recipes {
            for section in &recipe.sections {
                for component in &section.components {
                    ret.push(component.clone());
                }
            }
        }

        ret
    }

    pub fn make_shopping_list(
        components: Vec<Component>,
    ) -> Result<String, IncompatibleComponentError> {
        let mut combined_components: Vec<Component> = Vec::new();
        let mut ingredient_ids: Vec<i64> = Vec::new();

        for component in components {
            if ingredient_ids.contains(&component.ingredient.id) {
                for (i, component_) in combined_components.clone().into_iter().enumerate() {
                    if component.ingredient.id != component_.ingredient.id {
                        continue;
                    }

                    combined_components[i] = (component_ + component.clone())?;
                    break;
                }
            } else {
                ingredient_ids.push(component.ingredient.id);
                combined_components.push(component);
            }
        }

        let mut shopping_list: Vec<String> = Vec::new();

        for component in combined_components {
            if component.measurements.len() == 0
                || component
                    .measurements
                    .clone()
                    .into_iter()
                    .all(|m| m.quantity == 0.0)
            {
                shopping_list.push(component.ingredient.display_singular);
            } else {
                let quantity_str = if component.measurements[0].quantity.fract() == 0.0 {
                    format!("{}", component.measurements[0].quantity as i64)
                } else {
                    format!("{:.2}", component.measurements[0].quantity)
                };

                let formatted_str = format!(
                    "{}: {} {}",
                    component.ingredient.display_singular,
                    quantity_str,
                    component.measurements[0].unit.abbreviation
                );

                shopping_list.push(formatted_str);
            }
        }

        Ok(shopping_list.join("\n"))
    }

    pub mod models {
        use std::ops::Add;

        use thiserror::Error;

        use crate::utils::numeric;
        use serde::{de, Deserialize, Deserializer};

        #[derive(Deserialize, Debug, Clone)]
        pub struct Unit {
            name: String,
            pub abbreviation: String,
        }

        fn parse_float<'de, D>(deserializer: D) -> Result<f64, D::Error>
        where
            D: Deserializer<'de>,
        {
            let numeric_str = String::deserialize(deserializer)?;
            let n_chars = numeric_str.split_whitespace().count();
            let parsed: Result<f64, _> = numeric_str.parse();

            if numeric_str.is_ascii() && parsed.is_ok() {
                // Normal number
                return Ok(parsed.unwrap());
            } else if n_chars > 1 && n_chars < 3 {
                // Mixed fraction
                let mut split = numeric_str.split_whitespace();

                let number_part: f64 = split.next().unwrap().parse().unwrap();
                let fraction_part: f64 =
                    numeric(&split.next().unwrap().chars().next().unwrap()).unwrap();

                debug_assert!(split.next().is_none());

                return Ok(number_part + fraction_part);
            } else if n_chars == 1 {
                numeric(&numeric_str.chars().next().unwrap())
                    .ok_or(de::Error::custom("Not a fraction"))
            } else {
                panic!("BIG PROBLEM: {}", numeric_str);
            }
        }

        #[derive(Deserialize, Debug, Clone)]
        pub struct Measurement {
            id: i64,
            #[serde(deserialize_with = "parse_float")]
            pub quantity: f64,
            pub unit: Unit,
        }

        #[derive(Deserialize, Debug, Clone)]
        pub struct Ingredient {
            pub id: i64,
            pub display_singular: String,
        }

        #[derive(Deserialize, Debug, Clone)]
        pub struct Component {
            pub ingredient: Ingredient,
            pub measurements: Vec<Measurement>,
        }

        #[derive(Clone, Debug, Eq, Error, PartialEq)]
        #[error("Components must have the same ingredients in order to add their amounts.")]
        pub struct IncompatibleComponentError;

        impl Add for Component {
            type Output = Result<Self, IncompatibleComponentError>;

            fn add(self, rhs: Self) -> Self::Output {
                if self.ingredient.id != rhs.ingredient.id {
                    return Err(IncompatibleComponentError);
                }

                let mut ret = Component {
                    ingredient: self.ingredient,
                    measurements: Vec::new(),
                };

                for measurement in &self.measurements {
                    for rhs_measurement in &rhs.measurements {
                        if measurement.unit.name != rhs_measurement.unit.name {
                            continue;
                        }

                        ret.measurements.push(Measurement {
                            id: measurement.id,
                            quantity: measurement.quantity + rhs_measurement.quantity,
                            unit: measurement.unit.clone(),
                        })
                    }
                }

                Ok(ret)
            }
        }

        #[derive(Deserialize, Debug)]
        pub struct Section {
            pub components: Vec<Component>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Tag {
            pub id: i64,
        }

        #[derive(Deserialize, Debug)]
        pub struct Recipe {
            pub name: String,
            pub id: i64,
            pub slug: String,
            pub sections: Vec<Section>,
            pub tags: Vec<Tag>,
        }

        #[derive(Deserialize, Debug)]
        pub struct RecipeList {
            pub count: i32,
            pub results: Vec<Recipe>,
        }
    }
}

pub mod utils {
    use crate::api;
    use crate::database::{get_recipe_tags, recipe_exists};
    use phf::phf_map;
    use sqlx::SqlitePool;
    use std::process::Command;
    use text_io::try_read;

    static NUMERIC: phf::Map<char, f64> = phf_map! {
        '¼' => 0.25,
        '½' => 0.5,
        '¾' => 0.75,
        '⅐' => 1_f64 / 7_f64,
        '⅑' => 1_f64 / 9_f64,
        '⅒' => 0.1,
        '⅓' => 1_f64 / 3_f64,
        '⅔' => 2_f64 / 3_f64,
        '⅕' => 0.2,
        '⅖' => 0.4,
        '⅗' => 0.6,
        '⅘' => 0.8,
        '⅙' => 1_f64 / 6_f64,
        '⅚' => 5_f64 / 6_f64,
        '⅛' => 0.125,
        '⅜' => 0.375,
        '⅝' => 0.625,
        '⅞' => 0.875,
        '⅟' => 1.0,
        '↉' => 0.0,
    };

    pub fn numeric(c: &char) -> Option<f64> {
        NUMERIC.get(c).copied()
    }

    #[cfg(target_os = "windows")]
    pub fn open_file(file_path: String) -> std::io::Result<()> {
        Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(file_path)
            .spawn()?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub fn open_file(file_path: String) -> std::io::Result<()> {
        Command::new("xdg-open").arg(file_path).spawn()?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub fn open_file(file_path: String) -> std::io::Result<()> {
        Command::new("open").arg(file_path).spawn()?;
        Ok(())
    }

    pub async fn remove_duplicate_recipes(
        recipes: Vec<api::Recipe>,
        pool: &SqlitePool,
    ) -> Result<Vec<api::Recipe>, sqlx::Error> {
        let mut unique_recipes: Vec<api::Recipe> = Vec::new();

        for recipe in recipes {
            if !recipe_exists(recipe.id, pool).await? {
                unique_recipes.push(recipe);
            }
        }

        Ok(unique_recipes)
    }

    pub fn validation_input<T>(prompt: Option<&str>, message_on_failure: Option<&str>) -> T
    where
        T: std::str::FromStr,
        T: std::fmt::Display,
        T::Err: std::fmt::Debug,
    {
        let prompt = prompt.unwrap_or("");
        let ret: T;

        loop {
            print!("{}", prompt);
            let n: Result<T, _> = try_read!();

            match n {
                Ok(r) => {
                    ret = r;
                    break;
                }
                Err(_) => {
                    match message_on_failure {
                        Some(ref s) => eprintln!("{}", s),
                        None => eprintln!("Your input could not be converted."),
                    };
                }
            }
        }

        return ret;
    }

    pub async fn get_matching_recipes(
        recipes: Vec<api::Recipe>,
        n_recipes: i64,
        pool: &SqlitePool,
    ) -> Result<Vec<api::Recipe>, sqlx::Error> {
        let mut scores: Vec<(api::Recipe, i64)> = Vec::new();

        for recipe in recipes {
            let mut recipe_score: i64 = 0;

            for tag in get_recipe_tags(recipe.id, pool).await? {
                recipe_score += tag.likes;
            }

            scores.push((recipe, recipe_score));
        }

        scores.sort_by(|a, b| a.1.cmp(&b.1));

        Ok(scores
            .into_iter()
            .map(|i| i.0)
            .rev()
            .take(n_recipes as usize)
            .collect())
    }

    pub mod models {
        use std::{fmt::Display, str::FromStr};

        #[derive(Debug, Clone, Copy)]
        pub enum Rating {
            Dislike = -1,
            None = 0,
            Like = 1,
            Love = 2,
        }

        impl Rating {
            pub fn value(&self) -> i64 {
                *self as i64
            }
        }

        impl FromStr for Rating {
            type Err = &'static str;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                use Rating::*;
                match s.to_lowercase().as_str() {
                    "dislike" => Ok(Dislike),
                    "none" => Ok(None),
                    "like" => Ok(Like),
                    "love" => Ok(Love),
                    _ => Err("Please enter dislike, none, like, or love."),
                }
            }
        }

        impl Display for Rating {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                use Rating::*;

                match self {
                    Dislike => write!(f, "dislike"),
                    None => write!(f, "none"),
                    Like => write!(f, "like"),
                    Love => write!(f, "love"),
                }
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Mode {
            Prepare = 0,
            Review = 1,
        }

        impl Mode {
            pub fn value(&self) -> i64 {
                *self as i64
            }
        }

        impl Into<Mode> for i64 {
            fn into(self) -> Mode {
                match self {
                    0 => Mode::Prepare,
                    1 => Mode::Review,
                    _ => panic!("`data` table contains a `mode` value other than 0 or 1"),
                }
            }
        }
    }
}

pub mod database {
    use crate::utils::models::Mode;
    use futures::future::join_all;
    pub use models::Recipe;
    use models::{Data, RecipeTag, Tag};
    use sqlx::{query, query_as, SqlitePool};

    pub async fn tables_exist(pool: &SqlitePool) -> bool {
        query!("SELECT * FROM data LIMIT 1")
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
            .is_some()
    }

    pub async fn create_tables(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        query!(
            "CREATE TABLE IF NOT EXISTS `tags`( \
                `id`    INT UNSIGNED NOT NULL PRIMARY KEY, \
                `likes` INT NOT NULL \
            )"
        )
        .execute(pool)
        .await?;
        query!(
            "CREATE TABLE IF NOT EXISTS `recipes`( \
                `id`   INT UNSIGNED NOT NULL PRIMARY KEY, \
                `name` VARCHAR(255) NOT NULL \
            )"
        )
        .execute(pool)
        .await?;
        query!(
            "CREATE TABLE IF NOT EXISTS `previous_recipes`( \
                `recipe_id`              INT UNSIGNED NOT NULL, \
                FOREIGN KEY(`recipe_id`) REFERENCES recipes(`id`) \
            )"
        )
        .execute(pool)
        .await?;
        query!(
            "CREATE TABLE IF NOT EXISTS `recipe_tags`( \
                `recipe_id`              INT UNSIGNED NOT NULL, \
                `tag_id`                 INT UNSIGNED NOT NULL, \
                FOREIGN KEY(`recipe_id`) REFERENCES recipes(`id`), \
                FOREIGN KEY(`tag_id`)    REFERENCES tags(`id`) \
            )"
        )
        .execute(pool)
        .await?;
        query!(
            "CREATE TABLE IF NOT EXISTS `data`( \
                `mode`   INT UNSIGNED NOT NULL DEFAULT 0, \
                `offset` INT UNSIGNED NOT NULL DEFAULT 0 \
            )"
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn populate_data_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        query!("INSERT INTO data DEFAULT VALUES")
            .execute(pool)
            .await?;

        Ok(())
    }

    pub async fn get_mode(pool: &SqlitePool) -> Result<Mode, sqlx::Error> {
        let data = query_as!(Data, "SELECT mode, offset FROM data LIMIT 1")
            .fetch_one(pool)
            .await?;

        Ok(data.mode)
    }

    pub async fn get_offset(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
        let data = query_as!(Data, "SELECT mode, offset FROM data LIMIT 1")
            .fetch_one(pool)
            .await?;

        Ok(data.offset)
    }

    pub async fn get_previous_recipes(pool: &SqlitePool) -> Result<Vec<Recipe>, sqlx::Error> {
        query_as!(Recipe, "SELECT recipes.id, recipes.name FROM recipes INNER JOIN previous_recipes ON recipes.id = previous_recipes.recipe_id")
            .fetch_all(pool)
            .await
    }

    pub async fn get_recipe_tags(
        recipe_id: i64,
        pool: &SqlitePool,
    ) -> Result<Vec<Tag>, sqlx::Error> {
        let tag_ids: Vec<i64> = query_as!(
            RecipeTag,
            "SELECT recipe_id, tag_id FROM recipe_tags WHERE recipe_id = $1",
            recipe_id
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|rt| rt.tag_id)
        .collect();

        let tag_futures = tag_ids
            .into_iter()
            .map(|tag_id| get_tag_by_id(tag_id, pool));

        join_all(tag_futures).await.into_iter().collect()
    }

    async fn get_tag_by_id(id: i64, pool: &SqlitePool) -> Result<Tag, sqlx::Error> {
        query_as!(Tag, "SELECT id, likes FROM tags WHERE id = $1 LIMIT 1", id)
            .fetch_one(pool)
            .await
    }

    pub async fn update_tag_likes(
        id: i64,
        value: i64,
        pool: &SqlitePool,
    ) -> Result<(), sqlx::Error> {
        query!(
            "UPDATE tags SET likes = likes + $1 WHERE id = $2",
            value,
            id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn delete_previous_recipes(pool: &SqlitePool) -> Result<(), sqlx::Error> {
        query!("DELETE FROM previous_recipes").execute(pool).await?;

        Ok(())
    }

    pub async fn set_mode(mode: Mode, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        let value = mode.value();
        query!("UPDATE data SET mode = $1", value)
            .execute(pool)
            .await?;

        Ok(())
    }

    pub async fn recipe_exists(recipe_id: i64, pool: &SqlitePool) -> Result<bool, sqlx::Error> {
        Ok(
            query!("SELECT * FROM recipes WHERE id = $1 LIMIT 1", recipe_id)
                .fetch_optional(pool)
                .await?
                .is_some(),
        )
    }

    pub async fn store_tag(tag_id: i64, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        query!(
            "INSERT OR IGNORE INTO tags (id, likes) VALUES ($1, 0)",
            tag_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }
    pub async fn store_recipe_tag_relationship(
        recipe_id: i64,
        tag_id: i64,
        pool: &SqlitePool,
    ) -> Result<(), sqlx::Error> {
        store_tag(tag_id, pool).await?;

        query!(
            "INSERT INTO recipe_tags (recipe_id, tag_id) VALUES ($1, $2)",
            recipe_id,
            tag_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn store_recipe(
        recipe: &crate::api::Recipe,
        pool: &SqlitePool,
    ) -> Result<(), sqlx::Error> {
        query!(
            "INSERT OR IGNORE INTO recipes (id, name) VALUES ($1, $2)",
            recipe.id,
            recipe.name,
        )
        .execute(pool)
        .await?;

        for tag in &recipe.tags {
            store_recipe_tag_relationship(recipe.id, tag.id, pool).await?;
        }

        Ok(())
    }

    pub async fn store_previous_recipe(
        recipe: &crate::api::Recipe,
        pool: &SqlitePool,
    ) -> Result<(), sqlx::Error> {
        query!(
            "INSERT INTO previous_recipes (recipe_id) VALUES ($1)",
            recipe.id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn increment_offset(n: i64, pool: &SqlitePool) -> Result<(), sqlx::Error> {
        query!("UPDATE data SET offset = offset+$1", n)
            .execute(pool)
            .await?;

        Ok(())
    }

    pub mod models {
        use crate::utils::models::Mode;
        use serde::Deserialize;
        use sqlx::FromRow;
        #[derive(FromRow, Debug, PartialEq, Eq)]
        pub struct Tag {
            pub id: i64,
            pub likes: i64,
        }

        #[derive(FromRow, Debug, PartialEq, Eq, Deserialize)]
        pub struct Recipe {
            pub id: i64,
            pub name: String,
        }

        #[derive(FromRow, Debug, PartialEq, Eq)]
        pub struct PreviousRecipe {
            pub recipe_id: i64,
        }

        #[derive(FromRow, Debug, PartialEq, Eq)]
        pub struct RecipeTag {
            pub recipe_id: i64,
            pub tag_id: i64,
        }

        #[derive(FromRow, Debug, PartialEq, Eq)]
        pub struct Data {
            pub mode: Mode,
            pub offset: i64,
        }
    }
}
