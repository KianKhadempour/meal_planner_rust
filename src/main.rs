use meal_planner::{
    api::{
        get_components, get_recipes_list, make_shopping_list, models::IncompatibleComponentError,
    },
    database::{
        self, create_data_table, data_table_exists, delete_previous_recipes, get_mode, get_offset,
        get_previous_recipes, get_recipe_tags, increment_offset, set_mode, store_previous_recipe,
        store_recipe, update_tag_likes,
    },
    utils::{
        get_matching_recipes,
        models::{Mode, Rating},
        open_file, remove_duplicate_recipes, validation_input,
    },
};
use spinoff::{spinners, Color, Spinner};
use sqlx::{self, sqlite::SqlitePoolOptions, SqlitePool};

use tokio::{fs::OpenOptions, io::AsyncWriteExt};

use chrono::Utc;

use std::env;
use thiserror::Error;

#[derive(Error, Debug)]
enum PrepareError {
    #[error("sql error")]
    SqlError(#[from] sqlx::Error),
    #[error("environment variable error")]
    EnvError(#[from] env::VarError),
    #[error("reqwests error")]
    ReqError(#[from] reqwest::Error),
    #[error("incompatible component error")]
    CmpError(#[from] IncompatibleComponentError),
    #[error("file error")]
    FileError(#[from] std::io::Error),
    #[error("dotenv error")]
    DotError(#[from] dotenv::Error),
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
        get_recipes_list(get_offset(pool).await?, 200, &string_key).await?,
        pool,
    )
    .await?;
    spinner.success("Done!");

    let recipes = get_matching_recipes(all_recipes, n_recipes, pool).await?;
    let components = get_components(&recipes);
    let shopping_list = make_shopping_list(components)?;

    let now = Utc::now();
    let today = now.date_naive();
    let time = now.format("%H:%M").to_string();

    // Shopping List
    let shopping_list_file_path = format!("shopping-list-{}.txt", today);
    let mut shopping_list_file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&shopping_list_file_path)
        .await?;
    let shopping_list_content = format!(
        "{}\n{}\n{}\n\n",
        time,
        "-".repeat(time.len()),
        shopping_list
    );
    shopping_list_file
        .write_all(shopping_list_content.as_bytes())
        .await?;

    // Recipes
    let recipes_file_path = format!("recipes-{}.txt", today);
    let mut recipes_file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&recipes_file_path)
        .await?;
    let recipes_content = format!(
        "{}\n{}\n{}\n\n",
        time,
        "-".repeat(time.len()),
        recipes
            .iter()
            .map(|r| format!("https://tasty.co/recipe/{}", r.slug))
            .collect::<Vec<_>>()
            .join("\n")
    );
    recipes_file.write_all(recipes_content.as_bytes()).await?;

    open_file(shopping_list_file_path)?;
    open_file(recipes_file_path)?;

    for recipe in recipes {
        store_recipe(&recipe, pool).await?;
        store_previous_recipe(&recipe, pool).await?;
    }

    increment_offset(n_recipes, pool).await?;
    set_mode(Mode::Review, pool).await?;

    Ok(())
}

async fn review(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let previous_recipes: Vec<database::Recipe> = get_previous_recipes(&pool).await?;

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
    dotenv::dotenv()?;

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect("sqlite://database.db?mode=rwc")
        .await?;

    if !data_table_exists(&pool).await? {
        create_data_table(&pool).await?;
    }

    let mode = get_mode(&pool).await?;

    if mode == Mode::Prepare {
        println!("Preparing");
        prepare(&pool).await?;
    } else {
        println!("Reviewing");
        review(&pool).await?;
    }

    Ok(())
}
