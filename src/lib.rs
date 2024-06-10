use futures::future::join_all;
use std::fmt::{Debug, Display};

use std::str::FromStr;

use sqlx::{query, query_as, FromRow, SqlitePool};
use text_io::try_read;

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

pub fn validation_input<T>(prompt: Option<&str>, message_on_failure: Option<&str>) -> T
where
    T: std::str::FromStr,
    T: std::fmt::Display,
    T::Err: Debug,
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
                    Some(ref s) => println!("{}", s),
                    None => println!("Your input could not be converted."),
                };
            }
        }
    }

    return ret;
}

#[derive(FromRow, Debug, PartialEq, Eq)]
pub struct Tag {
    pub id: i64,
    likes: i64,
}

#[derive(FromRow, Debug, PartialEq, Eq)]
pub struct Recipe {
    pub id: i64,
    pub name: String,
}

#[derive(FromRow, Debug, PartialEq, Eq)]
pub struct PreviousRecipe {
    recipe_id: i64,
}

#[derive(FromRow, Debug, PartialEq, Eq)]
pub struct RecipeTag {
    recipe_id: i64,
    tag_id: i64,
}

#[derive(FromRow, Debug, PartialEq, Eq)]
pub struct Data {
    mode: Mode,
    offset: i64,
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

pub async fn get_recipe_tags(recipe_id: i64, pool: &SqlitePool) -> Result<Vec<Tag>, sqlx::Error> {
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

    join_all(tag_futures)
        .await
        .into_iter()
        .collect::<Result<Vec<Tag>, sqlx::Error>>()
}

async fn get_tag_by_id(id: i64, pool: &SqlitePool) -> Result<Tag, sqlx::Error> {
    query_as!(Tag, "SELECT id, likes FROM tags WHERE id = $1 LIMIT 1", id)
        .fetch_one(pool)
        .await
}

pub async fn update_tag_likes(id: i64, value: i64, pool: &SqlitePool) -> Result<(), sqlx::Error> {
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
