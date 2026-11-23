using System;
using System.IO;
using System.Text;
using System.Collections.Generic;
using System.Threading.Tasks;
using Playnite.SDK;
using Playnite.SDK.Models;
using Playnite.SDK.Plugins;

public class FixtureLibrary:LibraryPlugin {
    public override Guid Id { get { return new Guid("11111111-1111-1111-1111-111111111111"); } }
    public override string Name { get { return "Host contract fixture"; } }
    public FixtureLibrary(IPlayniteAPI api):base(api) {
        if(System.Windows.Application.Current!=null) throw new Exception("Unexpected WPF application");
    }
    public override IEnumerable<PlayController> GetPlayActions(GetPlayActionsArgs args) {
        if(args.Game.GameId=="paths") {
            var expected=Playnite.SDK.Data.Serialization.FromJsonFile<Dictionary<string,string>>(Path.Combine(args.Game.InstallDirectory,"paths.json"));
            Require(string.Equals(PlayniteApi.Paths.ApplicationPath,expected["program"],StringComparison.OrdinalIgnoreCase),"Wrong program root: "+PlayniteApi.Paths.ApplicationPath+" expected "+expected["program"]);
            Require(PlayniteApi.Paths.IsPortable==(!File.Exists(Path.Combine(expected["program"],"unins000.exe"))),"Wrong portable mode");
            var settings=Path.Combine(PlayniteApi.Paths.ExtensionsDataPath,Id.ToString(),"config.json");
            Require(File.ReadAllText(settings)=="split-settings-marker","Wrong settings root");
            File.WriteAllText(settings,"worker-only-change");
        }
        if(args.Game.GameId=="serialization") CheckSerialization(args.Game.InstallDirectory);
        if(args.Game.GameId=="unsupported") { var ignored=PlayniteApi.Database.Games; }
        if(args.Game.GameId.StartsWith("automatic")) {
            yield return new AutomaticPlayController(args.Game) {
                Type=AutomaticPlayActionType.File, Path=System.IO.Path.Combine(args.Game.InstallDirectory,"WindowFixture.exe"),
                Arguments="exit \""+System.IO.Path.Combine(args.Game.InstallDirectory,"window.hwnd")+"\"",
                WorkingDir=args.Game.InstallDirectory, TrackingMode=args.Game.GameId=="automatic-directory"?TrackingMode.Directory:TrackingMode.Process, TrackingPath=args.Game.InstallDirectory
            };
        } else {
            yield return new FixtureController(args.Game);
            if(args.Game.GameId=="multiple") yield return new FixtureController(args.Game);
        }
    }
    public class JsonValue { public int Value; }
    static void Require(bool condition,string message) { if(!condition) throw new Exception(message); }
    static MemoryStream JsonStream(string text) { return new MemoryStream(Encoding.UTF8.GetBytes(text)); }
    static void CheckSerialization(string directory) {
        const string json="{\"Value\":42}";
        JsonValue value; Exception error;
        var file=Path.Combine(directory,"serializer.json");
        File.WriteAllText(file,json);
        Require(Playnite.SDK.Data.Serialization.TryFromJson(json,out value) && value.Value==42,"JSON out value");
        Require(Playnite.SDK.Data.Serialization.TryFromJson(json,out value,out error) && value.Value==42 && error==null,"JSON success error");
        Require(!Playnite.SDK.Data.Serialization.TryFromJson("{",out value,out error) && value==null && error!=null,"JSON error out value");
        Require(!Playnite.SDK.Data.Serialization.TryFromJson("{",out value) && value==null,"JSON two-argument failure");
        Require(Playnite.SDK.Data.Serialization.TryFromJson("null",out value) && value==null,"JSON null is valid");
        Require(Playnite.SDK.Data.Serialization.TryFromJsonFile(file,out value) && value.Value==42,"File out value");
        Require(Playnite.SDK.Data.Serialization.TryFromJsonFile(file,out value,out error) && value.Value==42 && error==null,"File success error");
        Require(!Playnite.SDK.Data.Serialization.TryFromJsonFile(file+".missing",out value,out error) && value==null && error is FileNotFoundException,"Missing file");
        File.WriteAllText(file,"{");
        Require(!Playnite.SDK.Data.Serialization.TryFromJsonFile(file,out value) && value==null,"Malformed file");
        var stream=JsonStream(json);
        Require(Playnite.SDK.Data.Serialization.TryFromJsonStream(stream,out value) && value.Value==42 && !stream.CanRead,"Stream out value and ownership");
        Require(Playnite.SDK.Data.Serialization.TryFromJsonStream(JsonStream(json),out value,out error) && value.Value==42 && error==null,"Stream success error");
        Require(!Playnite.SDK.Data.Serialization.TryFromJsonStream(JsonStream("{"),out value,out error) && value==null && error!=null,"Stream error");
        Require(!Playnite.SDK.Data.Serialization.TryFromJsonStream(JsonStream("{"),out value) && value==null,"Stream two-argument failure");
        Require(Playnite.SDK.Data.Serialization.FromJsonStream<JsonValue>(JsonStream(json)).Value==42,"Direct stream read");
    }
}
class FixtureController:PlayController {
    public FixtureController(Game game):base(game) {}
    public override void Play(PlayActionArgs args) {
        if(Game.GameId=="throw") throw new Exception("Intentional dispatch failure");
        if(Game.GameId=="pending") return;
        if(Game.GameId=="early-stop") { InvokeOnStopped(new GameStoppedEventArgs()); return; }
        if(Game.GameId=="crash") Environment.Exit(17);
        Task.Run(async ()=> {
            InvokeOnStarted(new GameStartedEventArgs { StartedProcessId=1234 });
            await Task.Delay(300);
            InvokeOnStopped(new GameStoppedEventArgs());
        });
    }
}

