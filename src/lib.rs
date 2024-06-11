pub mod api {
    use models::{Component, Recipe, RecipeList};

    const BASE_URL: &str = "tasty.p.rapidapi.com";

    #[tokio::main]
    pub async fn get_recipes_list(
        offset: i64,
        size: i64,
        rapidapi_key: &str,
    ) -> Result<Vec<Recipe>, reqwest::Error> {
        let client = reqwest::Client::new();
        let response = client
            .get(BASE_URL.to_owned() + "/recipes/list")
            .header("x-rapidapi-key", rapidapi_key)
            .header("x-rapidapi-host", "tasty.p.rapidapi.com")
            .form(&[("from", offset), ("size", size)])
            .send()
            .await?
            .json::<RecipeList>()
            .await;

        match response {
            Ok(recipe_list) => Ok(recipe_list.results),
            Err(e) => {
                eprintln!("Failed to access the Tasty API!");
                Err(e)
            }
        }
    }

    pub async fn get_components(recipes: Vec<Recipe>) -> Result<Vec<Component>, sqlx::Error> {
        todo!()
    }

    pub async fn make_shopping_list(components: Vec<Component>) -> String {
        todo!()
    }

    mod models {
        use std::ops::Add;

        use thiserror::Error;

        use serde::Deserialize;

        #[derive(Deserialize, Debug, Clone)]
        pub struct Unit {
            name: String,
        }

        #[derive(Deserialize, Debug)]
        pub struct Measurement {
            id: i64,
            quantity: f64,
            unit: Unit,
        }

        #[derive(Deserialize, Debug)]
        pub struct Ingredient {
            id: i64,
        }

        #[derive(Deserialize, Debug)]
        pub struct Component {
            ingredient: Ingredient,
            measurements: Vec<Measurement>,
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
            components: Vec<Component>,
        }

        #[derive(Deserialize, Debug)]
        pub struct Recipe {
            name: String,
            id: i64,
            slug: String,
            sections: Vec<Section>,
        }

        #[derive(Deserialize, Debug)]
        pub struct RecipeList {
            pub count: i32,
            pub results: Vec<Recipe>,
        }
    }
}

pub mod utils {
    use crate::database::models::Recipe;
    use crate::database::{get_recipe_tags, recipe_exists};
    use sqlx::SqlitePool;
    use text_io::try_read;
    pub async fn remove_duplicate_recipes(
        recipes: Vec<Recipe>,
        pool: &SqlitePool,
    ) -> Result<Vec<Recipe>, sqlx::Error> {
        let mut unique_recipes: Vec<Recipe> = Vec::new();

        for recipe in recipes {
            if !recipe_exists(&recipe, pool).await? {
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
        recipes: Vec<Recipe>,
        n_recipes: i64,
        pool: &SqlitePool,
    ) -> Result<Vec<Recipe>, sqlx::Error> {
        let mut scores: Vec<(Recipe, i64)> = Vec::new();

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
    use models::{Data, Recipe, RecipeTag, Tag};
    use sqlx::{query, query_as, SqlitePool};

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

    pub async fn recipe_exists(recipe: &Recipe, pool: &SqlitePool) -> Result<bool, sqlx::Error> {
        Ok(
            query!("SELECT * FROM recipes WHERE id = $1 LIMIT 1", recipe.id)
                .fetch_optional(pool)
                .await?
                .is_some(),
        )
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
