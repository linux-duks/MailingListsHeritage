"""Integration tests for the anonymizer pipeline.

Validates the full flow: read parquet → anonymize identities → write parquet → verify.
"""

import polars as pl
from mlh_anonymizer.list_processor import parse_mail_at


def test_parse_mail_at_anonymizes_identities(input_parquet_dir, tmp_path):
    output_dir = tmp_path / "output"
    output_dir.mkdir()

    parse_mail_at("test_list", str(input_parquet_dir), str(output_dir))

    result_path = output_dir / "__main_dataset" / "test_list" / "data.parquet"
    assert result_path.exists(), "Anonymized output parquet should exist"

    df = pl.read_parquet(result_path)

    anonymized_from = df["from"].to_list()
    assert anonymized_from[0] == (
        "314dafacd900b2b9600fcecb7fbe4e7e6ebb816e"
        " <6ff30822aa7eae3ea817fa890fe02af8daba27e0>"
    )
    assert anonymized_from[1] == (
        "be2f58e9d777054a2174379de0cf0e863a95a57e"
        " <74abc462788f589acab8dfca2089c384958b6c2f>"
    )
    assert anonymized_from[2] == "a903c5ba062d4545b12ec5a2ff0a8509294c74a3"
    assert anonymized_from[3] == "fa2a1ee9662b85918dc8e5c4eff9c61ccff72038"
    assert anonymized_from[4] == (
        "dc69c2c6cdb5b56c466501d4ee161b09b529e886"
        " <10444bb1af05df1b8d5340beca0f78b338e12ff2>"
    )

    anonymized_to = df["to"].to_list()
    assert anonymized_to[0] == ["9a57905485c324f775450013a37baae982a06fa7"]
    assert anonymized_to[1] == [
        "6ff30822aa7eae3ea817fa890fe02af8daba27e0",
        "be2f58e9d777054a2174379de0cf0e863a95a57e"
        " <74abc462788f589acab8dfca2089c384958b6c2f>",
    ]
    assert anonymized_to[2] == ["f567b3165e2d074e26eab4098aaaac30ac989ebf"]
    assert anonymized_to[3] == ["0f7b7fff8a4c6ddcfe6f0ba3d32e990bfc741c38"]
    assert anonymized_to[4] == [
        "6c93090978e1e6a88c49bf58a6b848002f7c3a7b",
        "1f00cf0f4590a093141a003a3b01b7fa2460e5d5",
    ]

    anonymized_cc = df["cc"].to_list()
    assert anonymized_cc[0] == ["a903c5ba062d4545b12ec5a2ff0a8509294c74a3"]
    assert anonymized_cc[1] == []
    assert anonymized_cc[2] == [
        "1bcbc931ab9b99f50419ded7816d2fdf02753f26",
        "eafb1a70d13f18974b88fd137e4d56ec028bb32f"
        " <b68d1974354ad8efed027e10f4752b08de7c7a01>",
    ]
    assert anonymized_cc[3] == ["#include <linux/version.h>"]
    assert anonymized_cc[4] == [
        "d126ed0c3b9b340d678c4000b68e22411725ac28",
        "5768d7d642b98673da1bf94295703c7f8033c7a4",
    ]

    trailers_list = df["trailers"].to_list()
    assert len(trailers_list[0]) == 1
    assert (
        trailers_list[0][0]["identification"]
        == "567f342ca3222a3c95bdfd21e2861e6b25b1cc9e"
        " <d01486ee33b2283893efd9ed8d48fb6215701542>"
    )
    assert trailers_list[0][0]["attribution"] == "Signed-off-by"

    assert trailers_list[1] == []

    assert len(trailers_list[2]) == 1
    assert (
        trailers_list[2][0]["identification"]
        == "95ec127e641efb19396c339e8de09353f567a31b"
        " <655d23d0e1deeb26e8d50b4998a3a10f7e681f71>"
    )
    assert trailers_list[2][0]["attribution"] == "Reported-by"

    assert trailers_list[3] == []

    assert len(trailers_list[4]) == 1
    assert trailers_list[4][0]["identification"] == "user(a)domain.com"
    assert trailers_list[4][0]["attribution"] == "Suggested-by"

    id_map_path = output_dir / "__id_map_from" / "test_list" / "data.parquet"
    assert id_map_path.exists(), "ID map dataset should exist"


def test_parse_mail_at_preserves_deterministic_hashing(input_parquet_dir, tmp_path):
    """Same input must produce identical output on every run."""
    output_a = tmp_path / "output_a"
    output_b = tmp_path / "output_b"
    output_a.mkdir()
    output_b.mkdir()

    parse_mail_at("test_list", str(input_parquet_dir), str(output_a))
    parse_mail_at("test_list", str(input_parquet_dir), str(output_b))

    df_a = pl.read_parquet(output_a / "__main_dataset" / "test_list" / "data.parquet")
    df_b = pl.read_parquet(output_b / "__main_dataset" / "test_list" / "data.parquet")

    assert df_a.equals(df_b), "Deterministic hashing: outputs must be identical"
