SELECT * FROM common.locale ORDER BY mkey;
SELECT column_name FROM information_schema.columns WHERE table_schema='player' AND table_name='skill_proto' ORDER BY 1;
SELECT column_name FROM information_schema.columns WHERE table_schema='player' AND table_name='mob_proto' AND column_name IN ('setraceflag','setimmuneflag','rank') ORDER BY 1;
SELECT 'mob_proto.size sample:', size, '| setRaceFlag:', setRaceFlag, '| rank:', rank FROM player.mob_proto WHERE vnum=101;
SELECT 'skill sample:', dwvnum, szname, szpointon, setaffectflag, eskillskilltype FROM player.skill_proto LIMIT 1;
