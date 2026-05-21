"""Test fixtures for the anonymizer integration tests."""

import polars as pl
import pytest


@pytest.fixture
def input_parquet_dir(tmp_path):
    """Create a parquet dataset simulating mlh_parser output.

    Produces a directory structure matching the parser's output schema,
    containing known identity data for verifying the anonymization pipeline.
    """
    mailing_list = "test_list"
    list_dir = tmp_path / mailing_list
    list_dir.mkdir()

    df = pl.DataFrame(
        {
            "from": [
                "Mon Mothma <mon.mothma@coruscant.senate>",
                "Miles O'Brien <miles.obrien@starfleet.local>",
                "video4linux-list@redhat.com",
                "user@sub.domain.example.com",
                "Joe Developer <joe@linux-foundation.org>",
            ],
            "to": [
                ["amd-gfx@lists.freedesktop.org"],
                [
                    "mon.mothma@coruscant.senate",
                    "Miles O'Brien <miles.obrien@starfleet.local>",
                ],
                ["dm-devel@redhat.com"],
                ["user+tag@domain.com"],
                ["user@my-domain.org", "user-name@my-domain.org"],
            ],
            "cc": [
                ["video4linux-list@redhat.com"],
                [],
                [
                    "linux-ppp@vger.kernel.org",
                    "David Woodhouse <taramyn.barcona@coruscant.senate>",
                ],
                ["#include <linux/version.h>"],
                ["user+suffix@domain.com", "valid@test.com"],
            ],
            "trailers": [
                [
                    {
                        "attribution": "Signed-off-by",
                        "identification": "Kathryn Janeway <kathryn.janeway@starfleet.local>",
                    }
                ],
                [],
                [
                    {
                        "attribution": "Reported-by",
                        "identification": "高倉健 <okarum@oni.club>",
                    }
                ],
                [],
                [
                    {
                        "attribution": "Suggested-by",
                        "identification": "user(a)domain.com",
                    }
                ],
            ],
        }
    )

    df.write_parquet(list_dir / "data.parquet")
    return tmp_path
