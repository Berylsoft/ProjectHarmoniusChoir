SELECT max("version") FROM "__migrations" HAVING COUNT(*) > 0;
