use cypher_rs::*;
fn main() {
    let q = parse(
        "MATCH (u:User)-[:FOLLOWS]->(f:User) \
         WHERE u.id = $uid \
         RETURN f.name AS name, f.created_at AS joined \
         ORDER BY joined DESC \
         LIMIT 10",
    ).unwrap();
    let p = plan(&q).unwrap();
    println!("{p}");
}
