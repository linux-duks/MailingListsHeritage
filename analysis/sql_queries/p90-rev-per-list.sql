-- this query acompanies the "revisions_analisis" script
--
-- check the P90 max patch version for each list and the global P90
--
-- Filter and group by list and untagged_subject (Matches the Polars df logic)
WITH patch_revisions AS (
    SELECT 
        list, 
        untagged_subject,
        MIN(date) AS min_date,
        MAX(date) AS max_date,
        COUNT(*) AS rev_count,
        -- Window function to count total patches per list
        SUM(COUNT(*)) OVER (PARTITION BY list) AS list_total_patches
    FROM 
        dataset
    WHERE 
        date BETWEEN CAST('2016-05-01' AS DATE) AND CAST('2026-05-01' AS DATE)
        AND (has_patch_tag OR has_rfc_tag)
        AND NOT has_response_tag
        AND NOT has_forward_tag
        AND untagged_subject IS NOT NULL
        AND untagged_subject != ''
    GROUP BY 
        list, 
        untagged_subject
)

--- P90 in each list ignoring lists with less than 2000 patches over the 10 years
SELECT 
    list as list,
    APPROX_PERCENTILE_CONT(rev_count, 0.90) AS p90_rev_count,
FROM 
    patch_revisions
-- WHERE 
--     list_total_patches > 10000 -- optional filter
GROUP BY 
    list

UNION ALL

-- P90 across all lists
SELECT
    'overall rev_count' as list,
    APPROX_PERCENTILE_CONT(rev_count, 0.90) AS p90_rev_count
FROM 
    patch_revisions
-- WHERE 
--     list_total_patches > 10000 -- optional filter
ORDER BY 
    p90_rev_count desc;


