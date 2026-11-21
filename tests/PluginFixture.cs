using System;
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

