use daroyan_core::db::entities::users::Model as User;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
#[schema(as = User)]
pub struct UserView {
    pub id: String,
    pub email: String,
    pub name: String,
    pub avatar_url: Option<String>,
}

impl From<User> for UserView {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            name: user.name,
            avatar_url: user.avatar_url,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[schema(as = Me)]
pub struct MeView {
    #[serde(flatten)]
    pub user: UserView,
    #[schema(example = "admin")]
    pub role: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = StartedSignIn)]
pub struct StartedSignInView {
    pub authorization_url: String,
}
