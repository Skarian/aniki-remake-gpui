using System;
using System.IO;
using System.Linq;
using LiteDB;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

public static class Export {
 public static readonly string[] Tables={"games","platforms","emulators","genres","companies","sources","features"};
 static string Stamp(string dir) { return string.Join("|",Directory.GetFiles(dir,"*.db").OrderBy(x=>x).Select(x=>{var f=new FileInfo(x);return f.Name+":"+f.Length+":"+f.LastWriteTimeUtc.Ticks;})); }
 public static int Main(string[] args) {
  try {
   string root=Path.GetFullPath(args[0]), library=args.Length>2?Path.GetFullPath(args[2]):Path.Combine(root,"library");
   string before=Stamp(library); var tables=new JObject();
   foreach(var table in Tables) {
    var rows=new JArray();var path=Path.Combine(library,table+".db");
    if(File.Exists(path)) using(var db=new LiteDatabase("Filename="+path+";Mode=ReadOnly"))
     foreach(var collection in db.GetCollectionNames()) foreach(var row in db.GetCollection(collection).FindAll()) rows.Add(JToken.Parse(LiteDB.JsonSerializer.Serialize(row)));
    tables[table]=rows;
   }
   if(before!=Stamp(library)) throw new IOException("Library changed during import; retaining previous snapshot.");
   Write(root,Path.Combine(root,"library"),tables,args[1]);return 0;
  } catch(Exception ex) { Console.Error.WriteLine(ex.Message);return 1; }
 }
 public static void Write(string root,string library,JObject tables,string output) {
  var data=new JObject(); data["schema"]=1;data["root"]=root;data["library"]=library;data["exported_utc"]=DateTime.UtcNow.ToString("o");data["collections"]=tables;
  string destination=Path.GetFullPath(output);Directory.CreateDirectory(Path.GetDirectoryName(destination));
  string tmp=destination+"."+Guid.NewGuid().ToString("N")+".tmp";
  File.WriteAllText(tmp,data.ToString(Formatting.None),new System.Text.UTF8Encoding(false));
  if(File.Exists(destination)) File.Replace(tmp,destination,null);else File.Move(tmp,destination);
 }
}
