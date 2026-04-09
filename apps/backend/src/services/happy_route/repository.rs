use super::state::HappyRouteState;

pub(super) struct HappyRouteWriteRepository<'a> {
    pub(super) store: &'a mut HappyRouteState,
}

impl<'a> HappyRouteWriteRepository<'a> {
    pub(super) fn new(store: &'a mut HappyRouteState) -> Self {
        Self { store }
    }
}
