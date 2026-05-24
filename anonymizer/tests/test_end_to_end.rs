//! End-to-end tests: Parquet in → anonymize → Parquet out (Polars native).

use polars::prelude::*;
use std::fs;
use tempfile::TempDir;

use anonymizer::process_mailing_list;

fn build_list_series(name: &str, data: &[Vec<&str>]) -> Series {
    let total: usize = data.iter().map(|v| v.len()).sum();
    let mut builder = ListStringChunkedBuilder::new(name.into(), data.len(), total);
    for vals in data {
        if vals.is_empty() {
            builder.append_values_iter(std::iter::empty());
        } else {
            builder.append_values_iter(vals.iter().copied());
        }
    }
    builder.finish().into_series()
}

fn build_struct_list_series(name: &str, data: &[Vec<(&str, &str)>]) -> Series {
    let n = data.len();
    let total: usize = data.iter().map(|v| v.len()).sum();

    let mut attrs: Vec<&str> = Vec::with_capacity(total);
    let mut idents: Vec<&str> = Vec::with_capacity(total);

    for row in data {
        for (attr, ident) in row {
            attrs.push(*attr);
            idents.push(*ident);
        }
    }

    let attr_s = Series::new("attribution".into(), attrs);
    let ident_s = Series::new("identification".into(), idents);

    let struct_chunked = StructChunked::from_series(
        PlSmallStr::from_static(""),
        total,
        [&attr_s, &ident_s].into_iter(),
    )
    .unwrap();

    let inner = struct_chunked.into_series();

    let mut offsets = Vec::with_capacity(n + 1);
    let mut off: i64 = 0;
    offsets.push(0);
    for row in data {
        off += row.len() as i64;
        offsets.push(off);
    }

    let inner_arr = inner.array_ref(0).clone();
    let inner_dtype = inner_arr.dtype().clone();

    use polars_arrow::array::ListArray;
    use polars_arrow::datatypes::{ArrowDataType, Field};
    use polars_arrow::offset::OffsetsBuffer;

    let item_field = Field::new(PlSmallStr::from_static("item"), inner_dtype, true);

    let offsets_buf = OffsetsBuffer::try_from(offsets).unwrap();

    let list_arr = ListArray::<i64>::try_new(
        ArrowDataType::LargeList(Box::new(item_field)),
        offsets_buf,
        inner_arr,
        None,
    )
    .unwrap();

    ListChunked::with_chunk(PlSmallStr::from_str(name), list_arr).into_series()
}

fn build_test_df() -> DataFrame {
    let from_s = Series::new(
        "from".into(),
        &[
            "Mon Mothma <mon.mothma@coruscant.senate>",
            "Miles O'Brien <miles.obrien@starfleet.local>",
            "video4linux-list@redhat.com",
            "user@sub.domain.example.com",
            "Joe Developer <joe@linux-foundation.org>",
        ][..],
    );

    let to_s = build_list_series(
        "to",
        &[
            vec!["amd-gfx@lists.freedesktop.org"],
            vec![
                "mon.mothma@coruscant.senate",
                "Miles O'Brien <miles.obrien@starfleet.local>",
            ],
            vec!["dm-devel@redhat.com"],
            vec!["user+tag@domain.com"],
            vec![
                "user@my-domain.org",
                "user-name@my-domain.org",
                "Dola Pirate <dola.pirate@ghibli.local>",
            ],
        ],
    );

    let cc_s = build_list_series(
        "cc",
        &[
            vec!["video4linux-list@redhat.com"],
            vec![],
            vec![
                "linux-ppp@vger.kernel.org",
                "David Woodhouse <taramyn.barcona@coruscant.senate>",
            ],
            vec!["#include <linux/version.h>"],
            vec!["user+suffix@domain.com", "valid@test.com"],
        ],
    );

    let trailers_s = build_struct_list_series(
        "trailers",
        &[
            vec![(
                "Signed-off-by",
                "Kathryn Janeway <kathryn.janeway@starfleet.local>",
            )],
            vec![],
            vec![("Reported-by", "高倉健 <okarum@oni.club>")],
            vec![],
            vec![("Suggested-by", "user(a)domain.com")],
        ],
    );

    let subject_s = Series::new(
        "subject".into(),
        &[
            "[PATCH] Fix kmod",
            "[PATCH] Fix warp drive",
            "[BUG] Replicator",
            "[PATCH] Remove version.h",
            "[PATCH] Fix typos",
        ][..],
    );

    let raw_body_s = Series::new(
        "raw_body".into(),
        &[
            // 0
            r#"Signed-off-by: Montgomery Scott <montgomery.scott@starfleet.local>
            ---
            test/accel_test.c | 2 +-
            1 file changed, 1 insertion(+), 1 deletion(-)

            diff --git a/test/accel_test.c b/test/accel_test.c
            index e7f0e8e..3e82e42 100644
            --- a/test/accel_test.c
            +++ b/test/accel_test.c
            @@ -54,7 +54,7 @@ struct acctest_context *acctest_init(int tflags)
                waitpkg = 0;
                cpuid(&leaf, unused, &waitpkg, unused + 1);
                if (waitpkg & 0x20) {
            -		dbg("umwait supported\n");
            +		dbg("umwait supported:WAITPKG CPUID.(EAX=07H,ECX=0H):ECX[bit 5]\n");
                    umwait_support = 1;
                }
            
            -- 
            2.43.0
            "#,
            // 1
            "",
            // 2
            r#"ghes_map function uses arch_apei_get_mem_attribute to get the
            protection bits for a given physical address. These protection
            bits are then used to map the physical address.

            Signed-off-by: Bix Caleen <bix.caleen@ferrix.local>
            ---
            arch/riscv/include/asm/acpi.h | 20 ++++++++++++++++++++
            1 file changed, 20 insertions(+)

            diff --git a/arch/riscv/include/asm/acpi.h b/arch/riscv/include/asm/acpi.h
            index 6e13695120bc..0c599452ef48 100644
            --- a/arch/riscv/include/asm/acpi.h
            +++ b/arch/riscv/include/asm/acpi.h
            @@ -27,6 +27,26 @@ extern int acpi_disabled;
            extern int acpi_noirq;
            extern int acpi_pci_disabled;
            
            +#ifdef	CONFIG_ACPI_APEI
            +/*
            + * acpi_disable_cmcff is used in drivers/acpi/apei/hest.c for disabling
            + * IA-32 Architecture Corrected Machine Check (CMC) Firmware-First mode
            + * with a kernel command line parameter "acpi=nocmcoff". But we don't
            + * have this IA-32 specific feature on ARM64, this definition is only
            + * for compatibility.
            + */
            +#define acpi_disable_cmcff 1
            +static inline pgprot_t arch_apei_get_mem_attribute(phys_addr_t addr)
            +{
            +	/*
            +	 * Until we have a way to look for EFI memory attributes.
            +	 */
            +	return PAGE_KERNEL;
            +}
            +#else /* CONFIG_ACPI_APEI */
            +#define acpi_disable_cmcff 0
            +#endif /* !CONFIG_ACPI_APEI */
            +
            static inline void disable_acpi(void)
            {
                acpi_disabled = 1;"#,
            //3
            "#include <linux/version.h>",
            // 4
            r#"Hi, I am Joe Developer
            Joe Developer
            Joe
            Developer
            this message if to Dola Pirate"#,
        ][..],
    );

    df![
        "from" => from_s,
        "to" => to_s,
        "cc" => cc_s,
        "trailers" => trailers_s,
        "raw_body" => raw_body_s,
        "subject" => subject_s,
    ]
    .unwrap()
}

fn build_expected_df() -> DataFrame {
    let from_s = Series::new(
        "from".into(),
        &[
            "314dafacd900b2b9600fcecb7fbe4e7e6ebb816e <6ff30822aa7eae3ea817fa890fe02af8daba27e0>",
            "be2f58e9d777054a2174379de0cf0e863a95a57e <74abc462788f589acab8dfca2089c384958b6c2f>",
            "a903c5ba062d4545b12ec5a2ff0a8509294c74a3",
            "fa2a1ee9662b85918dc8e5c4eff9c61ccff72038",
            "dc69c2c6cdb5b56c466501d4ee161b09b529e886 <10444bb1af05df1b8d5340beca0f78b338e12ff2>",
        ][..],
    );

    let to_s = build_list_series(
        "to",
        &[
            vec!["9a57905485c324f775450013a37baae982a06fa7"],
            vec![
                "6ff30822aa7eae3ea817fa890fe02af8daba27e0",
                "be2f58e9d777054a2174379de0cf0e863a95a57e <74abc462788f589acab8dfca2089c384958b6c2f>",
            ],
            vec!["f567b3165e2d074e26eab4098aaaac30ac989ebf"],
            vec!["0f7b7fff8a4c6ddcfe6f0ba3d32e990bfc741c38"],
            vec![
                "6c93090978e1e6a88c49bf58a6b848002f7c3a7b",
                "1f00cf0f4590a093141a003a3b01b7fa2460e5d5",
                "bbf72f9be375b097db86f21719ad705e1ff5550d <5c4ab0af0a92c3675f25902c5265692b18444407>",
            ],
        ],
    );

    let cc_s = build_list_series(
        "cc",
        &[
            vec!["a903c5ba062d4545b12ec5a2ff0a8509294c74a3"],
            vec![],
            vec![
                "1bcbc931ab9b99f50419ded7816d2fdf02753f26",
                "eafb1a70d13f18974b88fd137e4d56ec028bb32f <b68d1974354ad8efed027e10f4752b08de7c7a01>",
            ],
            vec!["#include <linux/version.h>"],
            vec![
                "d126ed0c3b9b340d678c4000b68e22411725ac28",
                "5768d7d642b98673da1bf94295703c7f8033c7a4",
            ],
        ],
    );

    let trailers_s = build_struct_list_series(
        "trailers",
        &[
            vec![(
                "Signed-off-by",
                "567f342ca3222a3c95bdfd21e2861e6b25b1cc9e <d01486ee33b2283893efd9ed8d48fb6215701542>",
            )],
            vec![],
            vec![(
                "Reported-by",
                "95ec127e641efb19396c339e8de09353f567a31b <655d23d0e1deeb26e8d50b4998a3a10f7e681f71>",
            )],
            vec![],
            vec![("Suggested-by", "user(a)domain.com")],
        ],
    );

    let subject_s = Series::new(
        "subject".into(),
        &[
            "[PATCH] Fix kmod",
            "[PATCH] Fix warp drive",
            "[BUG] Replicator",
            "[PATCH] Remove version.h",
            "[PATCH] Fix typos",
        ][..],
    );

    let raw_body_s = Series::new(
        "raw_body".into(),
        &[
            // 0
            r#"Signed-off-by: 2bb146f933735341437e6ccf70d4bc5812074ba5 <3eb428c6d4a46342ab1213e873e5d167e5752b5d>
            ---
            test/accel_test.c | 2 +-
            1 file changed, 1 insertion(+), 1 deletion(-)

            diff --git a/test/accel_test.c b/test/accel_test.c
            index e7f0e8e..3e82e42 100644
            --- a/test/accel_test.c
            +++ b/test/accel_test.c
            @@ -54,7 +54,7 @@ struct acctest_context *acctest_init(int tflags)
                waitpkg = 0;
                cpuid(&leaf, unused, &waitpkg, unused + 1);
                if (waitpkg & 0x20) {
            -		dbg("umwait supported\n");
            +		dbg("umwait supported:WAITPKG CPUID.(EAX=07H,ECX=0H):ECX[bit 5]\n");
                    umwait_support = 1;
                }
            
            -- 
            2.43.0
            "#,
            // 1
            "",
            // 2
            r#"ghes_map function uses arch_apei_get_mem_attribute to get the
            protection bits for a given physical address. These protection
            bits are then used to map the physical address.

            Signed-off-by: b10df05f15430a11be07e6e091856e2834b3bbf9 <1637c9e13355cf357c1427675ddb4fa12f270b1a>
            ---
            arch/riscv/include/asm/acpi.h | 20 ++++++++++++++++++++
            1 file changed, 20 insertions(+)

            diff --git a/arch/riscv/include/asm/acpi.h b/arch/riscv/include/asm/acpi.h
            index 6e13695120bc..0c599452ef48 100644
            --- a/arch/riscv/include/asm/acpi.h
            +++ b/arch/riscv/include/asm/acpi.h
            @@ -27,6 +27,26 @@ extern int acpi_disabled;
            extern int acpi_noirq;
            extern int acpi_pci_disabled;
            
            +#ifdef	CONFIG_ACPI_APEI
            +/*
            + * acpi_disable_cmcff is used in drivers/acpi/apei/hest.c for disabling
            + * IA-32 Architecture Corrected Machine Check (CMC) Firmware-First mode
            + * with a kernel command line parameter "acpi=nocmcoff". But we don't
            + * have this IA-32 specific feature on ARM64, this definition is only
            + * for compatibility.
            + */
            +#define acpi_disable_cmcff 1
            +static inline pgprot_t arch_apei_get_mem_attribute(phys_addr_t addr)
            +{
            +	/*
            +	 * Until we have a way to look for EFI memory attributes.
            +	 */
            +	return PAGE_KERNEL;
            +}
            +#else /* CONFIG_ACPI_APEI */
            +#define acpi_disable_cmcff 0
            +#endif /* !CONFIG_ACPI_APEI */
            +
            static inline void disable_acpi(void)
            {
                acpi_disabled = 1;"#,
            // 3
            "#include <linux/version.h>",
            // 4
            r#"Hi, I am Joe Developer
            Joe Developer
            Joe
            Developer
            this message if to Dola Pirate"#,
        ][..],
    );

    df![
        "from" => from_s,
        "to" => to_s,
        "cc" => cc_s,
        "trailers" => trailers_s,
        "raw_body" => raw_body_s,
        "subject" => subject_s,
    ]
    .unwrap()
}

fn write_test_parquet(dir: &std::path::Path, df: &mut DataFrame) {
    let path = dir.join("test_data.parquet");
    let file = fs::File::create(&path).unwrap();
    ParquetWriter::new(file)
        .with_compression(ParquetCompression::Zstd(None))
        .finish(df)
        .unwrap();
}

fn read_parquet_as_df(path: &std::path::Path) -> DataFrame {
    let file = fs::File::open(path).unwrap();
    ParquetReader::new(file).finish().unwrap()
}

fn read_trailers_identification(df: &DataFrame) -> Vec<Option<String>> {
    let col = df.column("trailers").unwrap();
    let list_ca = col.list().unwrap();
    let inner = list_ca.get_inner();
    let inner_struct = inner.struct_().unwrap();
    let ident = inner_struct.field_by_name("identification").unwrap();
    let ident_str = ident.str().unwrap();

    let mut ids = Vec::new();
    let mut ident_idx = 0;
    for i in 0..list_ca.len() {
        match list_ca.get_as_series(i) {
            Some(sub) => {
                if sub.is_empty() {
                    ids.push(None);
                } else {
                    ids.push(Some(ident_str.get(ident_idx).unwrap_or("").to_string()));
                    ident_idx += sub.len();
                }
            }
            None => {
                ids.push(None);
            }
        }
    }
    ids
}

#[test]
fn test_parse_mail_at() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let list_name = "test_list3";
    let list_input_dir = input_dir.path().join(list_name);
    fs::create_dir_all(&list_input_dir).unwrap();

    let mut df = build_test_df();
    write_test_parquet(&list_input_dir, &mut df);

    let res = process_mailing_list(list_name, input_dir.path(), output_dir.path(), 2);

    assert!(
        res.is_ok(),
        "process_mailing_list failed with error {}",
        res.err().unwrap()
    );

    let output_parquet = output_dir
        .path()
        .join("dataset")
        .join(list_name)
        .join("list_data.parquet");

    let out_df = read_parquet_as_df(&output_parquet);
    let expected_df = build_expected_df();

    assert_eq!(out_df.height(), expected_df.height());
    assert_eq!(out_df.width(), expected_df.width());

    // Compare each column
    for name in expected_df.get_column_names() {
        let out_col = out_df.column(name).unwrap().as_materialized_series();
        let exp_col = expected_df.column(name).unwrap().as_materialized_series();

        match exp_col.dtype() {
            DataType::String => {
                let out_str: Vec<String> = out_col
                    .str()
                    .unwrap()
                    .into_iter()
                    .map(|o| o.unwrap_or("").to_string())
                    .collect();
                let exp_str: Vec<String> = exp_col
                    .str()
                    .unwrap()
                    .into_iter()
                    .map(|o| o.unwrap_or("").to_string())
                    .collect();
                assert_eq!(out_str, exp_str, "Column '{}' mismatch", name);
            }
            DataType::List(inner) if matches!(inner.as_ref(), DataType::String) => {
                let out_list = out_col.list().unwrap();
                let exp_list = exp_col.list().unwrap();
                assert_eq!(
                    out_list.len(),
                    exp_list.len(),
                    "Column '{}' list length mismatch",
                    name
                );
                for i in 0..out_list.len() {
                    let out_s = out_list.get_as_series(i);
                    let exp_s = exp_list.get_as_series(i);
                    match (out_s, exp_s) {
                        (Some(o), Some(e)) => {
                            let o_vals: Vec<String> = o
                                .str()
                                .unwrap()
                                .into_iter()
                                .map(|v| v.unwrap_or("").to_string())
                                .collect();
                            let e_vals: Vec<String> = e
                                .str()
                                .unwrap()
                                .into_iter()
                                .map(|v| v.unwrap_or("").to_string())
                                .collect();
                            assert_eq!(o_vals, e_vals, "Column '{}' row {} mismatch", name, i);
                        }
                        (None, None) => {}
                        _ => panic!("Column '{}' row {} null mismatch", name, i),
                    }
                }
            }
            DataType::List(_) => {
                // Trailers (list of struct) — compare identification field
                let out_ids = read_trailers_identification(&out_df);
                let exp_ids = read_trailers_identification(&expected_df);
                assert_eq!(out_ids, exp_ids, "Column '{}' trailers mismatch", name);
            }
            other => {
                panic!("Unexpected dtype {} for column '{}'", other, name);
            }
        }
    }
}

#[test]
fn test_process_mailing_list_output_path() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    // Bare name input — output should use bare name
    {
        let list_name = "test_list_bare";
        let list_input_dir = input_dir.path().join(list_name);
        fs::create_dir_all(&list_input_dir).unwrap();
        let mut df = build_test_df();
        write_test_parquet(&list_input_dir, &mut df);

        process_mailing_list(list_name, input_dir.path(), output_dir.path(), 0).unwrap();

        let expected_path = output_dir
            .path()
            .join("dataset")
            .join(list_name)
            .join("list_data.parquet");
        assert!(
            expected_path.exists(),
            "bare name: expected output at {}",
            expected_path.display()
        );
    }

    // Hive format input (list=name) — output should NOT get list=list=name
    {
        let list_name = "hive_test";
        let hive_dir_name = format!("list={}", list_name);
        let list_input_dir = input_dir.path().join(&hive_dir_name);
        fs::create_dir_all(&list_input_dir).unwrap();
        let mut df = build_test_df();
        write_test_parquet(&list_input_dir, &mut df);

        process_mailing_list(&hive_dir_name, input_dir.path(), output_dir.path(), 0).unwrap();

        let expected_path = output_dir
            .path()
            .join("dataset")
            .join(&hive_dir_name)
            .join("list_data.parquet");
        assert!(
            expected_path.exists(),
            "hive name: expected output at {}",
            expected_path.display()
        );

        let wrong_path = output_dir
            .path()
            .join("dataset")
            .join(format!("list={}", hive_dir_name))
            .join("list_data.parquet");
        assert!(
            !wrong_path.exists(),
            "double list= prefix should not exist at {}",
            wrong_path.display()
        );
    }
}

#[test]
fn test_large_batch_200k() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let list_name = "large_list";
    let list_input_dir = input_dir.path().join(list_name);
    fs::create_dir_all(&list_input_dir).unwrap();

    let n_rows = 200_000usize;
    let identity = "Cassian Andor <cassian@kenari.fx>";
    let expected_hash =
        "2fc62ca334e8876315efff90f95af4cd922079de <b81f9a424d4117c707de2839b73cf83afbc1b32f>";

    // Build a DataFrame with 200k rows, all same identity in "from"
    let from_vals: Vec<&str> = std::iter::repeat_n(identity, n_rows).collect();
    let from_s = Series::new("from".into(), from_vals.as_slice());

    let empty_list_200k: Vec<Vec<&str>> = std::iter::repeat_n(Vec::new(), n_rows).collect();
    let to_s = build_list_series("to", &empty_list_200k);
    let cc_s = build_list_series("cc", &empty_list_200k);

    let body_vals: Vec<&str> = std::iter::repeat_n("no identity here", n_rows).collect();
    let raw_body_s = Series::new("raw_body".into(), body_vals.as_slice());

    let subject_vals: Vec<&str> = std::iter::repeat_n("[PATCH] test", n_rows).collect();
    let subject_s = Series::new("subject".into(), subject_vals.as_slice());

    let empty_trailers: Vec<Vec<(&str, &str)>> = std::iter::repeat_n(Vec::new(), n_rows).collect();
    let trailers_s = build_struct_list_series("trailers", &empty_trailers);

    let mut df = df![
        "from" => from_s,
        "to" => to_s,
        "cc" => cc_s,
        "trailers" => trailers_s,
        "raw_body" => raw_body_s,
        "subject" => subject_s,
    ]
    .unwrap();

    write_test_parquet(&list_input_dir, &mut df);

    process_mailing_list(list_name, input_dir.path(), output_dir.path(), 0).unwrap();

    let output_parquet = output_dir
        .path()
        .join("dataset")
        .join(list_name)
        .join("list_data.parquet");

    let out_df = read_parquet_as_df(&output_parquet);

    // Verify row count is preserved
    assert_eq!(
        out_df.height(),
        n_rows,
        "row count should be preserved after anonymization"
    );

    // Verify the from column was anonymized (all rows same hash)
    let from_col = out_df.column("from").unwrap().as_materialized_series();
    let from_values: Vec<String> = from_col
        .str()
        .unwrap()
        .into_iter()
        .map(|o| o.unwrap_or("").to_string())
        .collect();

    assert_eq!(from_values.len(), n_rows);
    for (i, val) in from_values.iter().enumerate() {
        assert_eq!(val, expected_hash, "row {i} should have hashed identity");
    }

    // Verify raw_body was NOT changed (no identity in it)
    let body_col = out_df.column("raw_body").unwrap().as_materialized_series();
    let body_values: Vec<String> = body_col
        .str()
        .unwrap()
        .into_iter()
        .map(|o| o.unwrap_or("").to_string())
        .collect();
    for val in &body_values {
        assert_eq!(
            val, "no identity here",
            "raw_body should be unchanged when no identity present"
        );
    }
}
