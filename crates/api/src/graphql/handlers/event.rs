use juniper::GraphQLObject;

#[derive(GraphQLObject, Debug, Clone)]
pub struct CtfCategory {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

#[derive(GraphQLObject, Debug, Clone)]
pub struct CtfDifficulty {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(GraphQLObject, Debug, Clone)]
pub struct EventConfig {
    pub event_name: String,
    pub front_page_md: String,
    pub rules_md: String,
    pub start_time: i32,
    pub end_time: i32,
    pub use_teams: bool,
    pub registration_start_time: Option<i32>,
    pub registration_end_time: Option<i32>,
    pub max_team_size: Option<i32>,
    pub scoreboard_freeze_time: Option<i32>,
    pub categories: Vec<CtfCategory>,
    pub difficulties: Vec<CtfDifficulty>,
}

pub async fn get_event_config(
    context: &crate::graphql::Context,
) -> juniper::FieldResult<EventConfig> {
    // This does not require authentication, it is considered public information.
    // TODO: Should we allow private events where this info is restricted?

    let mut client = context.repo_client();

    let request = tonic::Request::new(crate::manager_api::GetEventConfigurationRequest {});

    let response = client.get_event_configuration(request).await?;

    let config = response.into_inner();

    Ok(EventConfig {
        event_name: config.event_name,
        front_page_md: config.front_page_md,
        rules_md: config.rules_md,
        start_time: config.start_time as i32,
        end_time: config.end_time as i32,
        use_teams: config.use_teams,
        registration_start_time: config.registration_start_time.map(|t| t as i32),
        registration_end_time: config.registration_end_time.map(|t| t as i32),
        max_team_size: config.max_team_size.map(|s| s as i32),
        scoreboard_freeze_time: config.scoreboard_freeze_time.map(|t| t as i32),
        categories: config
            .categories
            .into_iter()
            .map(|(id, c)| CtfCategory {
                id,
                name: c.name,
                description: c.description,
                color: c.color,
            })
            .collect(),
        difficulties: config
            .difficulties
            .into_iter()
            .map(|(id, d)| CtfDifficulty {
                id,
                name: d.name,
                color: d.color,
            })
            .collect(),
    })
}
