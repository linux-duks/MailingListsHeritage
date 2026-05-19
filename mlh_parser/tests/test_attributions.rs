mod common;

use common::parse_trailers_file;
use mlh_parser::email_reader::{decode_mail, get_body};
use std::fs;

use mlh_parser::extractors::extract_attributions;

const SYZBOT_MAIL: &str = r#"
syzbot has found a reproducer for the following crash on:

HEAD commit:    79c3ba32 Merge tag 'drm-fixes-2019-06-07-1' of git://anong..
git tree:       upstream
console output: https://syzkaller.appspot.com/x/log.txt?x=1201b971a00000
kernel config:  https://syzkaller.appspot.com/x/.config?x=60564cb52ab29d5b
dashboard link: https://syzkaller.appspot.com/bug?extid=2ff1e7cb738fd3c41113
compiler:       gcc (GCC) 9.0.0 20181231 (experimental)
syz repro:      https://syzkaller.appspot.com/x/repro.syz?x=14a3bf51a00000
C reproducer:   https://syzkaller.appspot.com/x/repro.c?x=120d19f2a00000

The bug was bisected to:

commit 0fff724a33917ac581b5825375d0b57affedee76
Author: Paul Kocialkowski <paul.kocialkowski@bootlin.com>
Date:   Fri Jan 18 14:51:13 2019 +0000

     drm/sun4i: backend: Use explicit fourcc helpers for packed YUV422 check

bisection log:  https://syzkaller.appspot.com/x/bisect.txt?x=1467550f200000
final crash:    https://syzkaller.appspot.com/x/report.txt?x=1667550f200000
console output: https://syzkaller.appspot.com/x/log.txt?x=1267550f200000

IMPORTANT: if you fix the bug, please add the following tag to the commit:
Reported-by: syzbot+2ff1e7cb738fd3c41113@syzkaller.appspotmail.com
Fixes: 0fff724a3391 ("drm/sun4i: backend: Use explicit fourcc helpers for
packed YUV422 check")
"#;

#[test]
fn test_syzbot_email() {
    let attr = extract_attributions(SYZBOT_MAIL);
    assert!(attr.is_empty());
}

const EXAMPLE_MAIL: &str = r#"
Email blabla
Signed-off-by: Example Contributor <example@contributor.com>
"#;

#[test]
fn test_correct_email() {
    let attr = extract_attributions(EXAMPLE_MAIL);
    assert_eq!(attr.len(), 1);
    assert_eq!(attr[0].attribution, "Signed-off-by");
    assert_eq!(
        attr[0].identification,
        "Example Contributor <example@contributor.com>"
    );
}

#[test]
fn test_email_trailers() {
    let directory = "./fixtures/";
    let pairs = common::list_fixture_pairs(directory, ".trailers.expected");

    if pairs.is_empty() {
        panic!("test cases missing")
    }

    for (body_file, email_file) in &pairs {
        let mail_bytes = fs::read(email_file).unwrap();

        let expected_trailers = parse_trailers_file(body_file);

        let mail = decode_mail(&mail_bytes).unwrap();
        let actual_body = get_body(&mail);

        let attr = extract_attributions(&actual_body);

        assert_eq!(
            attr.len(),
            expected_trailers.len(),
            "Trailer count mismatch for {:?}",
            email_file
        );

        for (id, trailer) in attr.iter().enumerate() {
            assert_eq!(
                trailer, &expected_trailers[id],
                "Attribution mismatch for {:?}",
                email_file
            );
        }
    }
}
