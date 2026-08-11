SELECT vnum, size, size+0, HEX(size) FROM player.mob_proto WHERE vnum=101;
SELECT size, COUNT(*) FROM player.mob_proto GROUP BY size ORDER BY 2 DESC;
SELECT COUNT(*) AS bad FROM player.mob_proto WHERE size NOT IN ('SMALL','MEDIUM','BIG');
