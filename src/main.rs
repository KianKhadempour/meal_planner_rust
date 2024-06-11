use meal_planner::{
    api::{get_components, get_recipes_list, make_shopping_list},
    database::{
        delete_previous_recipes, get_mode, get_offset, get_previous_recipes, get_recipe_tags,
        models::Recipe, set_mode, update_tag_likes,
    },
    utils::{
        get_matching_recipes,
        models::{Mode, Rating},
        remove_duplicate_recipes, validation_input,
    },
};
use spinoff::{spinners, Color, Spinner};
use sqlx::{self, sqlite::SqlitePoolOptions, SqlitePool};

use std::env;

#[derive(Debug)]
enum PrepareError {
    SqlError(sqlx::Error),
    EnvError(env::VarError),
    ReqError(reqwest::Error),
}

impl From<sqlx::Error> for PrepareError {
    fn from(err: sqlx::Error) -> Self {
        Self::SqlError(err)
    }
}

impl From<env::VarError> for PrepareError {
    fn from(err: env::VarError) -> Self {
        Self::EnvError(err)
    }
}

impl From<reqwest::Error> for PrepareError {
    fn from(err: reqwest::Error) -> Self {
        Self::ReqError(err)
    }
}

async fn prepare(pool: &SqlitePool) -> Result<(), PrepareError> {
    let key = env::var("TASTY_API_KEY");
    let string_key: String;

    match key {
        Ok(s) => {
            string_key = s;
        }
        Err(e) => {
            eprintln!("Please set the TASTY_API_KEY environment variable to your Tasty API key and try again.");
            return Err(e.into());
        }
    };

    let n_recipes: i64 = validation_input(Some("How many recipes do you want? "), None);

    let mut spinner = Spinner::new(spinners::Arc, "Searching recipes...", Color::Blue);
    let all_recipes = remove_duplicate_recipes(
        get_recipes_list(get_offset(pool).await?, 200, &string_key)?, // Currently broken, needs to use other `Recipe` type.
        pool,
    )
    .await?;
    spinner.success("Done!");

    let recipes = get_matching_recipes(all_recipes, n_recipes, pool).await?;
    let components = get_components(recipes).await?;
    let shopping_list = make_shopping_list(components);

    Ok(())
}

async fn review(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let previous_recipes: Vec<Recipe> = get_previous_recipes(&pool).await?;

    for recipe in previous_recipes {
        let rating: Rating = validation_input(
            Some(&format!(
                "How did you like {} (dislike, none, like, or love)? ",
                recipe.name
            )),
            Some("Please enter a dislike, none, like, or love."),
        );

        for tag in get_recipe_tags(recipe.id, pool).await? {
            update_tag_likes(tag.id, rating.value(), pool).await?;
        }
    }

    delete_previous_recipes(pool).await?;
    set_mode(Mode::Prepare, pool).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), PrepareError> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite://database.db")
        .await?;

    let mode = get_mode(&pool).await?;

    if mode == Mode::Prepare {
        println!("Preparing");
        prepare(&pool).await?
    } else {
        println!("Reviewing");
        review(&pool).await?
    }

    Ok(())
}
