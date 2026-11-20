//! A generated mock used the way a test would use it.

#![allow(clippy::unwrap_used, clippy::panic)]

use tpt_dev_mock::mock;

/// A temperature sensor.
#[mock]
pub trait Sensor {
    /// Reads a channel.
    fn read(&self, channel: u8) -> u16;
    /// Resets the sensor.
    fn reset(&mut self);
    /// The sensor's label.
    fn label(&self) -> String;
}

/// The code under test: it reads two channels and sums them.
fn total(sensor: &dyn Sensor) -> u32 {
    u32::from(sensor.read(1)) + u32::from(sensor.read(2))
}

#[test]
fn matched_expectations_answer_calls() {
    let mock = MockSensor::new();
    mock.read.expect().with(|a| a.0 == 1).times(1).returns(100);
    mock.read.expect().with(|a| a.0 == 2).times(1).returns(23);
    assert_eq!(total(&mock), 123);
    assert_eq!(mock.read.calls(), 2);
    mock.verify();
}

#[test]
fn returns_with_sees_the_channel() {
    let mock = MockSensor::new();
    mock.read.expect().returns_with(|(ch,)| u16::from(ch) * 10);
    assert_eq!(mock.read(4), 40);
}

#[test]
fn mutable_methods_are_recorded() {
    let mut mock = MockSensor::new();
    mock.reset.expect().times(1).returns(());
    mock.reset();
    mock.verify();
}

#[test]
#[should_panic(expected = "was called 0 times, expected 1")]
fn verify_catches_a_missing_call() {
    let mock = MockSensor::new();
    mock.label.expect().times(1).returns(String::from("temp"));
    mock.verify();
}

#[test]
#[should_panic(expected = "unexpected call to `read`")]
fn unexpected_calls_panic() {
    let mock = MockSensor::new();
    let _ = mock.read(9);
}

#[test]
fn default_mock_has_no_expectations() {
    let mock = MockSensor::default();
    mock.verify();
    assert_eq!(mock.label.calls(), 0);
}

/// A source whose item and error types are bound by the attribute.
#[mock(Item = u32, Error = String)]
pub trait Source {
    /// The item type.
    type Item;
    /// The error type.
    type Error;
    /// The next item, if any.
    fn next_item(&mut self) -> Option<Self::Item>;
    /// Fetches the current item, or an error.
    fn fetch(&self, key: u8) -> Result<Self::Item, Self::Error>;
}

/// Generic code that depends on the trait through a bound.
fn sum_items<S: Source<Item = u32>>(source: &mut S) -> u32 {
    let mut total = 0;
    while let Some(item) = source.next_item() {
        total += item;
    }
    total
}

#[test]
fn associated_types_are_substituted_in_methods() {
    let mut mock = MockSource::new();
    mock.next_item.expect().times(1).returns(Some(4));
    mock.next_item.expect().times(1).returns(Some(6));
    mock.next_item.expect().times(1).returns(None);
    assert_eq!(sum_items(&mut mock), 10);
    mock.verify();
}

#[test]
fn associated_result_types_are_recorded() {
    let mock = MockSource::new();
    mock.fetch
        .expect()
        .with(|a| a.0 == 3)
        .times(1)
        .returns(Err(String::from("missing")));
    assert_eq!(mock.fetch(3), Err(String::from("missing")));
    mock.verify();
}

#[test]
fn the_mock_is_usable_as_a_trait_object() {
    let mut mock = MockSource::new();
    mock.next_item.expect().returns_with(|()| Some(9));
    let source: &mut dyn Source<Item = u32, Error = String> = &mut mock;
    assert_eq!(source.next_item(), Some(9));
}
