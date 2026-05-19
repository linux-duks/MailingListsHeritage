mod common;

use common::parse_patches_file;
use mlh_parser::email_reader::{decode_mail, get_body};
use mlh_parser::extractors::extract_patches;
use std::fs;

const NO_PATCH_MAIL: &str = r#"
syzbot has found a reproducer for the following crash on:

HEAD commit:    79c3ba32 Merge tag 'drm-fixes-2019-06-07-1' of git://anong..
git tree:       upstream
console output: https://syzkaller.appspot.com/x/log.txt?x=1201b971a00000
"#;

#[test]
fn test_no_patch_in_email() {
    let output = extract_patches(NO_PATCH_MAIL);
    assert!(output.is_empty());
}

const SINGLE_PATCH_MAIL: &str = "\
Interfaces for when a new domain in the crashdump kernel needs some
values from the panicked kernel's context entries.

Signed-off-by: Bill Sumner 
---
 drivers/iommu/intel-iommu.c | 46 +++++++++++++++++++++++++++++++++++++++++++++
 1 file changed, 46 insertions(+)

diff --git a/drivers/iommu/intel-iommu.c b/drivers/iommu/intel-iommu.c
index 73afed4..bde8f22 100644
--- a/drivers/iommu/intel-iommu.c
+++ b/drivers/iommu/intel-iommu.c
@@ -348,6 +348,7 @@ static inline int first_pte_in_page(struct dma_pte *pte)
#endif /* CONFIG_CRASH_DUMP */
--
2.0.0-rc0";

#[test]
fn test_single_patch_full_format() {
    let output = extract_patches(SINGLE_PATCH_MAIL);
    assert_eq!(output.len(), 1);
}

const MULTIPLE_PATCHES_MAIL: &str = "\
This is the first patch in the series. It fixes a minor typo.
Signed-off-by: Joe Developer <joe@example.com>
---
 drivers/net/ethernet/intel/e1000/e1000_main.c | 1 +
 1 file changed, 1 insertion(+)

diff --git a/drivers/net/ethernet/intel/e1000/e1000_main.c b/drivers/net/ethernet/intel/e1000/e1000_main.c
index a2d9b23..b3f1c40 100644
--- a/drivers/net/ethernet/intel/e1000/e1000_main.c
+++ b/drivers/net/ethernet/intel/e1000/e1000_main.c
@@ -512,6 +512,7 @@ static void e1000_reset_task(struct work_struct *work)
     /* Fix the typo in the previous comment */
+     /* New line of code */
     e1000_reset(adapter);
 }
-- 
2.30.0


This is the second patch. It adds a function.
Signed-off-by: Jane Developer <jane@example.com>
---
 include/linux/random.h | 11 +++++++++++
 1 file changed, 11 insertions(+)

diff --git a/include/linux/random.h b/include/linux/random.h
index e2c9a1d..f8e3f4c 100644
--- a/include/linux/random.h
+++ b/include/linux/random.h
@@ -5,6 +5,17 @@
 #include <linux/types.h>

+/**
+ * pr_get_random_int - Get a cryptographically secure random integer
+ *
+ * Returns a 32-bit random integer.
+ */
+static inline u32 pr_get_random_int(void)
+{
+    return get_random_u32();
+}
+
 extern void get_random_bytes(void *buf, int nbytes);
 extern void get_random_bytes_arch(void *buf, int nbytes);
-- 
2.30.0";

#[test]
fn test_multiple_patches_in_email() {
    let output = extract_patches(MULTIPLE_PATCHES_MAIL);
    assert_eq!(output.len(), 2);
}

const PATCH_NO_SPACE_BEFORE_GIT: &str = "\
Interfaces for when a new domain in the crashdump kernel needs some
values from the panicked kernel's context entries.

Signed-off-by: Bill Sumner 
---
 drivers/iommu/intel-iommu.c | 46 +++++++++++++++++++++++++++++++++++++++++++++
 1 file changed, 46 insertions(+)

diff --git a/drivers/iommu/intel-iommu.c b/drivers/iommu/intel-iommu.c
index 73afed4..bde8f22 100644
--- a/drivers/iommu/intel-iommu.c
+++ b/drivers/iommu/intel-iommu.c
@@ -348,6 +348,7 @@ static inline int first_pte_in_page(struct dma_pte *pte)
#endif /* CONFIG_CRASH_DUMP */
--
2.0.0-rc0";

#[test]
fn test_mail_without_space_before_git_version() {
    let output = extract_patches(PATCH_NO_SPACE_BEFORE_GIT);
    assert_eq!(output.len(), 1);
}

const PATCH_MANY_SPACES_BEFORE_GIT: &str = "\
Interfaces for when a new domain in the crashdump kernel needs some
values from the panicked kernel's context entries.

Signed-off-by: Bill Sumner 
---
 drivers/iommu/intel-iommu.c | 46 +++++++++++++++++++++++++++++++++++++++++++++
 1 file changed, 46 insertions(+)

diff --git a/drivers/iommu/intel-iommu.c b/drivers/iommu/intel-iommu.c
index 73afed4..bde8f22 100644
--- a/drivers/iommu/intel-iommu.c
+++ b/drivers/iommu/intel-iommu.c
@@ -348,6 +348,7 @@ static inline int first_pte_in_page(struct dma_pte *pte)
#endif /* CONFIG_CRASH_DUMP */
--              
2.0.0-rc0";

#[test]
fn test_mail_with_many_spaces_before_git_version() {
    let output = extract_patches(PATCH_MANY_SPACES_BEFORE_GIT);
    assert_eq!(output.len(), 1);
}

#[test]
fn test_patch_emails() {
    let directory = "./fixtures/";
    let pairs = common::list_fixture_pairs(directory, ".code.expected");

    if pairs.is_empty() {
        panic!("test cases missing")
    }
    for (patches_file, email_file) in &pairs {
        let mail_bytes = fs::read(email_file).unwrap();
        let expected_patches = parse_patches_file(patches_file);

        let mail = decode_mail(&mail_bytes).unwrap();
        let body = get_body(&mail);
        let acctual_patches = extract_patches(&body);

        for (id, patch) in acctual_patches.iter().enumerate() {
            assert_eq!(
                *patch, *expected_patches[id],
                "Patch mismatch for '{}' in {:?}",
                id, email_file
            );
        }
    }
}
