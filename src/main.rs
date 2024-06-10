use meal_planner::{
    delete_previous_recipes, get_mode, get_previous_recipes, get_recipe_tags, set_mode,
    update_tag_likes, validation_input, Mode, Rating, Recipe,
};
use sqlx::{self, sqlite::SqlitePoolOptions, SqlitePool};

async fn prepare(pool: &SqlitePool) -> Result<(), sqlx::Error> {
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
async fn main() -> Result<(), sqlx::Error> {
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
