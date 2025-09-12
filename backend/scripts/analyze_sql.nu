#!/usr/bin/env nu

let migrations =
    | ls "./src/database/migration/migrations/"
    | each { $in.name };

let migrations_sql =
    | $migrations
    | each { cat $in }
    | str join "\n;\n";

ls src/**/*.sql 
| where { $in.name not-in $migrations }
| each {
    {
        sql: (cat $in.name),
        name: $in.name
    }
}
| where { $in.sql !~ "__migrations" and $in.sql !~ "--# analyze ignore"}
| each {
    let sql = $migrations_sql + ";\nEXPLAIN QUERY PLAN " + $in.sql + ";\n";
    $in | insert eqp ($sql | sqlite3)
}
| where { |it|
    ($it.eqp =~ "(?i)SCAN" and $it.eqp !~ "(?i)SCAN CONSTANT ROW")
    | $in or ($it.eqp =~ "USE TEMP B-TREE")
}
| each {
    print $"(ansi g)($in.name)"
    print $"(ansi reset)($in.sql)"
    print $"(ansi g)eqp:"
    print $"(ansi reset)($in.eqp)"
    print ""
}
| ignore
