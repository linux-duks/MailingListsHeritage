"""Tests for the anonymizer main module."""

import pytest

from mlh_anonymizer.anonymizer import anonymize_string


string_test_cases = [
    # formed identity
    (
        "From: Mon Mothma <mon.mothma@coruscant.senate>",
        "From: 314dafacd900b2b9600fcecb7fbe4e7e6ebb816e <6ff30822aa7eae3ea817fa890fe02af8daba27e0>",
    ),
    (
        "Miles O'Brien <miles.obrien@starfleet.local>",
        "be2f58e9d777054a2174379de0cf0e863a95a57e <74abc462788f589acab8dfca2089c384958b6c2f>",
    ),
    # malformed identity (should not happen in header columns, as the parser fixes it)
    (
        "From: Mon Mothma mon.mothma@coruscant.senate",
        "From: Mon Mothma <6ff30822aa7eae3ea817fa890fe02af8daba27e0>",
    ),
    # email with "dash"
    (
        "amd-gfx@lists.freedesktop.org",
        "9a57905485c324f775450013a37baae982a06fa7",
    ),
    # email missing tld"
    (
        "amd-gfx@freedesktop",
        "9a99ca8e28a341ffac83afcb7d393175dc806608",
    ),
    # list of emails
    (
        "To: mon.mothma@coruscant.senate, amd-gfx@lists.freedesktop.org, miles.obrien@starfleet.local",
        "To: 6ff30822aa7eae3ea817fa890fe02af8daba27e0, 9a57905485c324f775450013a37baae982a06fa7, 74abc462788f589acab8dfca2089c384958b6c2f",
    ),
    # multi line string
    (
        """From: Kathryn Janeway <kathryn.janeway@starfleet.local>

        Straight forward conversions to CONFIG_MODULE; many drivers
        include <linux/kmod.h> conditionally and then don't have any
        other conditional code so remove it from those.

        Signed-off-by: Kathryn Janeway <kathryn.janeway@starfleet.local>
        Cc: video4linux-list@redhat.com
        Cc: David Woodhouse <taramyn.barcona@coruscant.senate>
        Cc: linux-ppp@vger.kernel.org
        Cc: dm-devel@redhat.com
        Signed-off-by: Alyssa Ogawa <alyssa.ogawa@starfleet.local>""",
        """From: 567f342ca3222a3c95bdfd21e2861e6b25b1cc9e <d01486ee33b2283893efd9ed8d48fb6215701542>

        Straight forward conversions to CONFIG_MODULE; many drivers
        include <linux/kmod.h> conditionally and then don't have any
        other conditional code so remove it from those.

        Signed-off-by: 567f342ca3222a3c95bdfd21e2861e6b25b1cc9e <d01486ee33b2283893efd9ed8d48fb6215701542>
        Cc: a903c5ba062d4545b12ec5a2ff0a8509294c74a3
        Cc: eafb1a70d13f18974b88fd137e4d56ec028bb32f <b68d1974354ad8efed027e10f4752b08de7c7a01>
        Cc: 1bcbc931ab9b99f50419ded7816d2fdf02753f26
        Cc: f567b3165e2d074e26eab4098aaaac30ac989ebf
        Signed-off-by: b1f386047221c342010c24fb02cdf3855f38ad46 <1098a4204bd8e6f3f4a48fdf24f9a94765f10786>""",
    ),
    # positive: edge case email formats
    (
        "user@sub.domain.example.com",
        "fa2a1ee9662b85918dc8e5c4eff9c61ccff72038",
    ),
    (
        "user@my-domain.org",
        "6c93090978e1e6a88c49bf58a6b848002f7c3a7b",
    ),
    (
        "user+tag@domain.com",
        "0f7b7fff8a4c6ddcfe6f0ba3d32e990bfc741c38",
    ),
    (
        "Joe Developer <joe@linux-foundation.org>",
        "dc69c2c6cdb5b56c466501d4ee161b09b529e886 <10444bb1af05df1b8d5340beca0f78b338e12ff2>",
    ),
    # obfuscated emails, handled by the parser, ignored here (as it would be too error prone)
    (
        "user(a)domain.com",
        "user(a)domain.com",
    ),
    (
        "user at domain.com",
        "user at domain.com",
    ),
    # negative: strings that look almost like identities but are not
    (
        "linux.kernel.org",
        "linux.kernel.org",
    ),
    (
        "#include <linux/version.h>",
        "#include <linux/version.h>",
    ),
    (
        "@@ -10,7 +10,6 @@",
        "@@ -10,7 +10,6 @@",
    ),
    (
        "2.20.1.7.g153144c",
        "2.20.1.7.g153144c",
    ),
    (
        # Multi Line string with patch and kanji names
        """
        Eliminate the follow versioncheck warning:

        ../rust/kernel/bindings_helper.h: 13 linux/version.h not needed.

        Reported-by: 高倉健 <okarum@oni.club>
        Signed-off-by: 綾瀬星子 <ayase.s@kumamori.local>
        ---
        rust/kernel/bindings_helper.h | 1 -
        1 file changed, 1 deletion(-)

        diff --git a/rust/kernel/bindings_helper.h b/rust/kernel/bindings_helper.h
        index 99a7d785ae01..a79f3f398b93 100644
        --- a/rust/kernel/bindings_helper.h
        +++ b/rust/kernel/bindings_helper.h
        @@ -10,7 +10,6 @@
        #include <linux/sysctl.h>
        #include <linux/uaccess.h>
        #include <linux/uio.h>
        -#include <linux/version.h>
        #include <linux/miscdevice.h>
        #include <linux/poll.h>
        #include <linux/mm.h>
        -- 
        2.20.1.7.g153144c""",
        """
        Eliminate the follow versioncheck warning:

        ../rust/kernel/bindings_helper.h: 13 linux/version.h not needed.

        Reported-by: 95ec127e641efb19396c339e8de09353f567a31b <655d23d0e1deeb26e8d50b4998a3a10f7e681f71>
        Signed-off-by: fa8b026a461f951f7a2d421204a26ed082786fc4 <50a5500306e5f4249370cefe79b249ad2c97a776>
        ---
        rust/kernel/bindings_helper.h | 1 -
        1 file changed, 1 deletion(-)

        diff --git a/rust/kernel/bindings_helper.h b/rust/kernel/bindings_helper.h
        index 99a7d785ae01..a79f3f398b93 100644
        --- a/rust/kernel/bindings_helper.h
        +++ b/rust/kernel/bindings_helper.h
        @@ -10,7 +10,6 @@
        #include <linux/sysctl.h>
        #include <linux/uaccess.h>
        #include <linux/uio.h>
        -#include <linux/version.h>
        #include <linux/miscdevice.h>
        #include <linux/poll.h>
        #include <linux/mm.h>
        -- 
        2.20.1.7.g153144c""",
    ),
    # negative cases: broken line detection would cause too much false positives
    (
        """mon.mothma@
        coruscant.senate""",
        """mon.mothma@
        coruscant.senate""",
    ),
    (
        """mon.
        mothma@
        coruscant.
        senate""",
        """mon.
        mothma@
        coruscant.
        senate""",
    ),
    (
        """From:
        Mon Mothma <
        mon.mothma@coruscant.senate
        >""",
        """From:
        Mon Mothma <
        mon.mothma@coruscant.senate
        >""",
    ),
]


@pytest.mark.parametrize("input_string, expected", string_test_cases)
def test_correct_email(input_string, expected) -> None:

    result = anonymize_string(input_string)
    assert result == expected, "Anonymized strings should match"
