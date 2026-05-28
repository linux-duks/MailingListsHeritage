-- check the oldest date per list 
--  use the _source_reference for 
--  manual evaluation with the check_git script (if PI),
--                          or check_nntp
SELECT
    date,
    list,
    _source_reference
FROM (
    SELECT
        list,
        date,
        _source_reference,
        ROW_NUMBER() OVER(PARTITION BY list ORDER BY date ASC) AS row_num
    FROM dataset
) subquery
WHERE row_num = 1 order by date;
