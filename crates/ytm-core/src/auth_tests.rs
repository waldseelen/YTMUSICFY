use super::*;

#[test]
fn compute_sapisidhash_matches_known_sha1_vector() {
    // sha1("1700000000 test_sapisid https://music.youtube.com")
    let token = compute_sapisidhash("test_sapisid", YTM_ORIGIN, 1_700_000_000);
    assert_eq!(
        token,
        "SAPISIDHASH 1700000000_34f6546f99ec18cec46e50e842f2dab45b9d2d7d"
    );
}

#[test]
fn extract_sapisid_prefers_secure_3papisid_and_strips_quotes() {
    let cookie = r#"SAPISID="old_val"; __Secure-3PAPISID="new_secure_val"; PREF=tz=UTC"#;
    assert_eq!(extract_sapisid(cookie).unwrap(), "new_secure_val");

    let only_sapisid = r#"PREF=tz=UTC; SAPISID="fallback_val""#;
    assert_eq!(extract_sapisid(only_sapisid).unwrap(), "fallback_val");

    assert!(matches!(
        extract_sapisid("PREF=tz=UTC; SID=abc"),
        Err(CoreError::MissingCookie)
    ));
}

#[test]
fn parse_auth_input_supports_browser_json_and_raw_headers() {
    let browser_json = r#"{
        "Cookie": "__Secure-3PAPISID=secret123; SID=abc",
        "User-Agent": "CustomUA/1.0",
        "X-Goog-AuthUser": "2",
        "X-Goog-PageId": "10987654321",
        "X-Goog-Visitor-Id": "CgtVisitor"
    }"#;
    let creds = parse_auth_input(browser_json).expect("parse browser.json");
    assert_eq!(creds.x_goog_authuser.as_deref(), Some("2"));
    assert_eq!(creds.x_goog_page_id.as_deref(), Some("10987654321"));
    assert_eq!(creds.x_goog_visitor_id.as_deref(), Some("CgtVisitor"));

    let raw_headers =
        "POST /youtubei/v1/browse HTTP/2\nCookie: SAPISID=raw_tok\nX-Goog-AuthUser: 1\n";
    let creds2 = parse_auth_input(raw_headers).expect("parse raw headers");
    assert_eq!(creds2.x_goog_authuser.as_deref(), Some("1"));
    let built = build_auth_headers(&creds2, 1_700_000_000).expect("build headers");
    assert!(built.iter().any(|(k, _)| k == "Authorization"));
    assert!(
        built
            .iter()
            .any(|(k, v)| k == "Cookie" && v.contains("SOCS=CAI"))
    );

    let chrome_two_line = "cookie\nSAPISID=chrome_sap; __Secure-3PAPISID=chrome_3p\nuser-agent\nChromeTest/1.0\nx-goog-authuser\n1\nx-goog-visitor-id\nVis123\n";
    let creds3 = parse_auth_input(chrome_two_line).expect("parse chrome 2-line headers");
    assert_eq!(creds3.user_agent, "ChromeTest/1.0");
    assert_eq!(creds3.x_goog_authuser.as_deref(), Some("1"));
    assert_eq!(creds3.x_goog_visitor_id.as_deref(), Some("Vis123"));
}

#[test]
fn netscape_cookies_to_header_preserves_httponly_and_filters_youtube() {
    let netscape = "\
# Netscape HTTP Cookie File
.google.com\tTRUE\t/\tTRUE\t1900000000\tIGNORE_ME\tfoo
.youtube.com\tTRUE\t/\tTRUE\t1900000000\tSAPISID\tsap_val
#HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t1900000000\t__Secure-1PSID\tpsid_val
music.youtube.com\tFALSE\t/\tTRUE\t1900000000\t__Secure-3PAPISID\t3p_val
";
    let header = netscape_cookies_to_header(netscape).expect("netscape cookies");
    assert!(header.contains("SAPISID=sap_val"));
    assert!(header.contains("__Secure-1PSID=psid_val"));
    assert!(header.contains("__Secure-3PAPISID=3p_val"));
    assert!(!header.contains("IGNORE_ME"));
}

#[test]
fn refresh_sapisidhash_in_headers_updates_authorization_timestamp() {
    let creds = parse_auth_input("Cookie: SAPISID=test_sapisid\n").expect("creds");
    let mut headers = build_auth_headers(&creds, 1_700_000_000).expect("headers");
    refresh_sapisidhash_in_headers(&mut headers, 1_700_000_042).expect("refresh");
    let auth = headers
        .iter()
        .find(|(k, _)| k == "Authorization")
        .map(|(_, v)| v.as_str())
        .expect("auth header");
    assert!(auth.starts_with("SAPISIDHASH 1700000042_"));
}
