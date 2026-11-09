using System;
using System.IO;
using System.Linq;
using System.Threading;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using Newtonsoft.Json.Converters;
using Playnite.SDK;
using Playnite.SDK.Events;
using Playnite.SDK.Plugins;

public sealed class LumenLiveExport : GenericPlugin {
 public override Guid Id {get{return new Guid("c4825127-e280-4484-9ae2-30c5a5d7a2fd");}}
 readonly IPlayniteAPI api; Timer timer; int running; bool started;
 string Output { get { return File.ReadAllText(Path.Combine(Path.GetDirectoryName(GetType().Assembly.Location),"destination.txt")).Trim(); } }
 public LumenLiveExport(IPlayniteAPI api):base(api){this.api=api;}
 public override void OnApplicationStarted(OnApplicationStartedEventArgs args) {
  started=true;
  api.Database.Games.ItemUpdated+=(s,e)=>Queue();api.Database.Games.ItemCollectionChanged+=(s,e)=>Queue();
  api.Database.Platforms.ItemUpdated+=(s,e)=>Queue();api.Database.Emulators.ItemUpdated+=(s,e)=>Queue();
  timer=new Timer(_=>Save(),null,500,15000);
 }
 void Queue(){if(started)timer.Change(400,15000);}
 void Save(){
  if(Interlocked.Exchange(ref running,1)!=0)return;
  try {
   var serializer=JsonSerializer.Create(new JsonSerializerSettings{Converters={new StringEnumConverter()},ReferenceLoopHandling=ReferenceLoopHandling.Ignore});
   var tables=new JObject();
   tables["games"]=JArray.FromObject(api.Database.Games.ToList(),serializer);
   tables["platforms"]=JArray.FromObject(api.Database.Platforms.ToList(),serializer);
   tables["emulators"]=JArray.FromObject(api.Database.Emulators.ToList(),serializer);
   tables["genres"]=JArray.FromObject(api.Database.Genres.ToList(),serializer);
   tables["companies"]=JArray.FromObject(api.Database.Companies.ToList(),serializer);
   tables["sources"]=JArray.FromObject(api.Database.Sources.ToList(),serializer);
   tables["features"]=JArray.FromObject(api.Database.Features.ToList(),serializer);
   Export.Write(api.Paths.ApplicationPath,api.Database.DatabasePath,tables,Output);
  } catch(Exception ex){LogManager.GetLogger().Warn(ex,"Aniki snapshot export failed; previous snapshot retained.");}
  finally{Interlocked.Exchange(ref running,0);}
 }
 public override void OnApplicationStopped(OnApplicationStoppedEventArgs args){started=false;if(timer!=null)timer.Dispose();}
}
