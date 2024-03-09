use bevy::prelude::*;
use openxr::AsyncRequestIdFB;
use crate::resources::XrSession;

pub struct RoomPlugin;

#[derive(Resource)]
struct RoomRequest(AsyncRequestIdFB);

impl Plugin for RoomPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, query_room);
    }
}

fn query_room(
    mut commands: Commands,
    xr_session: Option<Res<XrSession>>,
    room_request: Option<Res<RoomRequest>>,
) {
    if xr_session.is_none() {
        return;
    }
    let session = xr_session.unwrap();
    if room_request.is_some() {
        info!("checking room request");
        let id = room_request.unwrap().0;
        let check_result = session.check_room_query(id);
        if let Ok(results) = check_result {
            info!("room query results: {:?}", results);
            commands.remove_resource::<RoomRequest>();
        } else {
            warn!("{:?}", check_result.unwrap_err());
        };
        return
    }
    info!("querying room");
    let request = session.query_room();
    if request.is_err() {
        warn!("error getting layout: {:?}", request.unwrap_err());
        return;
    }
    let req = request.unwrap();
    info!("room request: {:?}", req);
    commands.insert_resource(RoomRequest(req));
}