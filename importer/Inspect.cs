using System;
using System.IO;
using LiteDB;
class Inspect {
 static void Main(string[] args) {
  foreach(var file in new[]{"games.db","platforms.db","emulators.db"}) {
   using(var db=new LiteDatabase("Filename="+Path.Combine(args[0],file)+";Mode=ReadOnly")) {
    foreach(var name in db.GetCollectionNames()) foreach(var doc in db.GetCollection(name).FindAll())
     Console.WriteLine(file+"\t"+LiteDB.JsonSerializer.Serialize(doc));
   }
  }
 }
}
