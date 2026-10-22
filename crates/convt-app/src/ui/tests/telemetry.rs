use super::*;

#[gpui_kit::test]
fn real_app_state_conversion_sign_in_and_persisted_opt_out(cx: &mut TestAppContext) {
    let f = Fixture::new(cx);
    f.api.answer_key(Ok(None));
    let input = f.png("private-customer-photo.png");
    let (url, got) = crate::telemetry::tests::sink(Duration::ZERO);
    let id = cx.read(|cx| f.app.read(cx).settings.install_id.clone().unwrap());
    let sender =
        crate::telemetry::Telemetry::local(id.clone(), false, url, Duration::from_secs(60));
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.telemetry.shutdown();
            s.telemetry = sender.clone();
            s.update_settings(|settings| settings.telemetry = true, cx);
            s.telemetry
                .capture(crate::telemetry::Event::Opened, &s.license);
            s.start_sign_in(cx);
        })
    });
    *f.api.exchange.lock().unwrap() = Ok(Session {
        email: "fake@convt.test".into(),
        token: "cvd_local_only".into(),
        user_id: Some("user_fake_55".into()),
    });
    let page = cx.opened_url().unwrap();
    let state = query(&page, "state");
    cx.update(|cx| super::super::route(auth_link(&state, "code=local"), cx));
    wait_until(cx, "signed in", |cx| {
        f.app.read(cx).account.sign_in == SignIn::Idle && f.app.read(cx).account.session.is_some()
    });
    wait_until(cx, "renewed", |cx| {
        matches!(f.app.read(cx).account.refresh, Refresh::Done(_))
    });
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.convert(
                std::slice::from_ref(&input),
                format_by_id("webp").unwrap(),
                &Options::default(),
                cx,
            )
            .unwrap();
        })
    });
    wait_until(cx, "conversion output", |cx| {
        f.app.read(cx).queue.active() == 0
    });
    let output = input.with_extension("webp");
    assert!(output.is_file());
    assert_eq!(image::open(&output).unwrap().width(), 4);
    let missing = f.dir.path().join("private-missing-file.png");
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.convert(
                &[missing],
                format_by_id("webp").unwrap(),
                &Options::default(),
                cx,
            )
            .unwrap();
        })
    });
    wait_until(cx, "failed conversion", |cx| {
        f.app.read(cx).queue.active() == 0
    });
    sender.shutdown();
    let (head, body) = got.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: application/json")
    );
    let events = body["batch"].as_array().unwrap();
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "$identify" && e["distinct_id"] == "user_fake_55")
    );
    let conversions: Vec<_> = events
        .iter()
        .filter(|e| e["event"].as_str().unwrap().starts_with("conversion_"))
        .collect();
    assert_eq!(conversions.len(), 4);
    for event in &conversions {
        assert_eq!(event["distinct_id"], "user_fake_55");
        assert_eq!(event["properties"]["from"], "png");
        assert_eq!(event["properties"]["to"], "webp");
        assert_eq!(event["properties"]["engine"], serde_json::json!(["image"]));
    }
    assert_eq!(conversions[1]["properties"]["success"], true);
    assert_eq!(conversions[3]["properties"]["success"], false);
    for private in ["private-customer", "private-missing", "fake@convt.test"] {
        assert!(!body.to_string().contains(private));
    }
    cx.update(|cx| {
        f.app
            .update(cx, |s, cx| s.update_settings(|s| s.telemetry = false, cx))
    });
    let saved = crate::settings::Settings::load(&f.dir.path().join("settings.toml")).unwrap();
    assert!(!saved.telemetry);
    assert_eq!(saved.install_id.as_deref(), Some(id.as_str()));
    println!(
        "APP_LOCAL_CAPTURE user_id=user_fake_55 events={} PNG->WebP=4x4 engine=image persisted_opt_out=false paths_and_error_text=absent",
        events.len()
    );
}

#[gpui_kit::test]
fn a_legacy_session_renews_identity_even_when_checked_today(cx: &mut TestAppContext) {
    let f = Fixture::signed_in(
        cx,
        Some(&pro_key("fake@convt.test", "2027-01-01")),
        "fake@convt.test",
    );
    f.api.answer_key(Ok(None));
    cx.update(|cx| {
        f.app.update(cx, |s, cx| {
            s.account.session.as_mut().unwrap().user_id = None;
            s.account.access = Some(Access::Pro);
            s.update_settings(
                |s| s.license_checked = Some(convt_license::date::from_days(client::today())),
                cx,
            );
            s.renew_if_due(cx);
        })
    });
    wait_until(cx, "verified identity", |cx| {
        f.app
            .read(cx)
            .account
            .session
            .as_ref()
            .unwrap()
            .user_id
            .as_deref()
            == Some("user_fake_55")
    });
    assert_eq!(f.api.calls().1, 1);
    cx.read(|cx| {
        assert_eq!(
            f.app
                .read(cx)
                .licensing
                .session()
                .unwrap()
                .user_id
                .as_deref(),
            Some("user_fake_55")
        )
    });
}

#[gpui_kit::test]
fn corrupt_settings_fail_closed_without_overwriting_an_opt_out(cx: &mut TestAppContext) {
    let original = "telemetry = false\nnotifications = broken";
    let f = Fixture::build(cx, |dir| {
        std::fs::write(dir.join("settings.toml"), original).unwrap();
        client::Config {
            enforce: false,
            public_key: None,
            build_date: BUILD_DATE.into(),
            trial_file: None,
            store: KeyStore::None,
        }
    });
    cx.read(|cx| assert!(!f.app.read(cx).settings.telemetry));
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("settings.toml")).unwrap(),
        original
    );
}
