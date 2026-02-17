use sandbox_quant::indicator::ema::Ema;

#[test]
fn basic_ema() {
    let mut ema = Ema::new(3);
    assert_eq!(ema.push(2.0), None);
    assert_eq!(ema.push(5.0), None);
    assert!(!ema.is_ready());

    let v = ema.push(8.0).unwrap();
    assert!((v - 5.0).abs() < f64::EPSILON);
    assert!(ema.is_ready());

    let v = ema.push(11.0).unwrap();
    assert!((v - 8.0).abs() < f64::EPSILON);

    let v = ema.push(14.0).unwrap();
    assert!((v - 11.0).abs() < f64::EPSILON);
}

#[test]
fn single_period() {
    let mut ema = Ema::new(1);
    let v = ema.push(42.0).unwrap();
    assert!((v - 42.0).abs() < f64::EPSILON);

    let v = ema.push(99.0).unwrap();
    assert!((v - 99.0).abs() < f64::EPSILON);
}

#[test]
fn value_without_push() {
    let mut ema = Ema::new(2);
    assert_eq!(ema.value(), None);
    ema.push(10.0);
    assert_eq!(ema.value(), None);
    ema.push(20.0);
    assert!((ema.value().unwrap() - 15.0).abs() < f64::EPSILON);
    ema.push(30.0);
    assert!((ema.value().unwrap() - 25.0).abs() < f64::EPSILON);
}

#[test]
#[should_panic(expected = "EMA period must be > 0")]
fn zero_period_panics() {
    Ema::new(0);
}
