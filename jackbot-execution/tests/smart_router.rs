use jackbot_execution::strategy::smart_router::SmartRouter;
use rust_decimal_macros::dec;

#[test]
fn router_enforces_limit() {
    let mut router = SmartRouter::new(dec!(5));
    assert!(router.record_execution(dec!(3)).is_ok());
    assert!(router.record_execution(dec!(2)).is_ok());
    assert!(router.record_execution(dec!(1)).is_err());
    assert_eq!(router.exposure(), dec!(5));
    router.reduce_exposure(dec!(2));
    assert!(router.record_execution(dec!(1)).is_ok());
}
