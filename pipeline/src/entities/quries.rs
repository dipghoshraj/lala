use sea_orm::entity::prelude::*;
use uuid::Uuid;


#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "queries")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Uuid,
    pub session_id: Uuid,
    pub query_text: String,
    pub query_embedding: Option<Vec<f32>>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Session,
    Answers,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Session => Entity::belongs_to(super::sessions::Entity)
                .from(Column::SessionId)
                .to(super::sessions::Column::Id)
                .into(),
            Self::Answers => Entity::has_many(super::answers::Entity).into(),
        }
    }
}