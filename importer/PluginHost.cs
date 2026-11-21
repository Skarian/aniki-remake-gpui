// Loads one installed library integration. No Playnite application or database host.
using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.Remoting.Messaging;
using System.Runtime.Remoting.Proxies;
using System.Threading;
using System.Windows.Threading;
using Playnite.SDK;
using Playnite.SDK.Models;
using Playnite.SDK.Plugins;
using Playnite.SDK.Data;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using Newtonsoft.Json.Serialization;

public static class AnikiPluginHost {
    internal static string Root, Scratch, PluginDir;
    internal static JsonSerializerSettings Json;
    static readonly object OutputLock = new object();
    static StreamWriter Output;
    // Entry must not touch SDK/Newtonsoft types until AssemblyResolve is installed.
    public static int Run(string json) {
        var values = new System.Web.Script.Serialization.JavaScriptSerializer().DeserializeObject(json) as System.Collections.Generic.Dictionary<string,object>;
        Root = Path.GetFullPath((string)values["root"]);
        var module = Path.GetFullPath((string)values["module"]);
        PluginDir = Path.GetDirectoryName(module);
        Scratch = Path.Combine(Path.GetTempPath(), "aniki-plugin-" + System.Diagnostics.Process.GetCurrentProcess().Id);
        Directory.CreateDirectory(Scratch);
        Output = new StreamWriter(Console.OpenStandardOutput()) { AutoFlush = true };
        AppDomain.CurrentDomain.AssemblyResolve += (s,e) => {
            var name = new AssemblyName(e.Name).Name + ".dll";
            foreach(var folder in new[]{PluginDir, Root}) {
                var file = Path.Combine(folder,name);
                if(File.Exists(file)) return Assembly.LoadFrom(file);
            }
            return null;
        };
        AppDomain.CurrentDomain.ProcessExit += (s,e) => Cleanup();
        int code = 0;
        var thread = new Thread(() => {
            try { Execute(json,module); }
            catch(Exception e) { Error(e.GetBaseException()); code=1; }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start(); thread.Join();
        Cleanup();
        return code;
    }
    static void Cleanup() { try { Directory.Delete(Scratch,true); } catch(IOException) {} catch(UnauthorizedAccessException) {} }
    [MethodImpl(MethodImplOptions.NoInlining)]
    static void Error(Exception e) { Send(new { @event="error", message=e.Message }); }
    static void Send(object data) { lock(OutputLock) Output.WriteLine("ANIKI:"+JsonConvert.SerializeObject(data)); }
    [MethodImpl(MethodImplOptions.NoInlining)]
    static void Execute(string json,string module) {
        Json = new JsonSerializerSettings { ContractResolver=new SdkResolver() };
        SynchronizationContext.SetSynchronizationContext(new DispatcherSynchronizationContext());
        var request=JObject.Parse(json);
        var game=request["game"].ToObject<Game>(JsonSerializer.Create(Json));
        // Constructors may migrate settings. Give them a private copy, never the original.
        var settings=Path.Combine(Root,"ExtensionsData",game.PluginId.ToString(),"config.json");
        var destination=Path.Combine(Scratch,"data",game.PluginId.ToString());
        Directory.CreateDirectory(destination);
        if(File.Exists(settings)) File.Copy(settings,Path.Combine(destination,"config.json"));
        typeof(Playnite.SDK.Data.Serialization).GetMethod("Init",BindingFlags.Static|BindingFlags.NonPublic)
            .Invoke(null,new[]{new ApiProxy(typeof(IDataSerializer)).GetTransparentProxy()});
        var api=(IPlayniteAPI)new ApiProxy(typeof(IPlayniteAPI)).GetTransparentProxy();
        typeof(API).GetProperty("Instance").GetSetMethod(true).Invoke(null,new object[]{api});
        typeof(ResourceProvider).GetMethod("SetGlobalProvider",BindingFlags.Static|BindingFlags.NonPublic).Invoke(null,new object[]{api.Resources});
        var sdk=typeof(Plugin).Assembly.GetName().Version;
        if(sdk.Major!=6) throw new NotSupportedException("This host supports Playnite SDK 6 only.");
        var types=Assembly.LoadFrom(module).GetTypes().Where(t=>!t.IsAbstract && typeof(LibraryPlugin).IsAssignableFrom(t)).ToArray();
        if(types.Length!=1) throw new NotSupportedException("Expected one library plugin in "+module);
        using(var plugin=(LibraryPlugin)Activator.CreateInstance(types[0],api)) {
            if(plugin.Id!=game.PluginId) throw new InvalidOperationException("Library plugin identity does not match this game.");
            var actions=plugin.GetPlayActions(new GetPlayActionsArgs{Game=game}).ToArray();
            try {
                if(actions.Length!=1) throw new NotSupportedException("This game has "+actions.Length+" plugin play actions; one is required.");
                var action=actions[0];
                var automatic=action as AutomaticPlayController;
                if(automatic!=null) {
                    var directory=automatic.TrackingMode==TrackingMode.Directory;
                    if(!directory && automatic.TrackingMode!=TrackingMode.Default && automatic.TrackingMode!=TrackingMode.Process)
                        throw new NotSupportedException("Unsupported automatic tracking: "+automatic.TrackingMode);
                    if(automatic.Type==AutomaticPlayActionType.Url && !directory)
                        throw new NotSupportedException("URL actions require directory tracking.");
                    Send(new { @event="automatic", plan=new {
                        kind=directory?"directory":"process", url=automatic.Type==AutomaticPlayActionType.Url, executable=automatic.Path,
                        arguments=automatic.Arguments??"", working_dir=automatic.WorkingDir??"",
                        install_dir=string.IsNullOrEmpty(automatic.TrackingPath)?game.InstallDirectory:automatic.TrackingPath,
                        initial_delay=automatic.InitialTrackingDelay, frequency=automatic.TrackingFrequency
                    }});
                    return;
                }
                var frame=new DispatcherFrame(); bool ended=false;
                EventHandler<GameStartedEventArgs> onStart=(s,e)=>Send(new { @event="started", pid=e.StartedProcessId });
                EventHandler<GameStoppedEventArgs> onStop=(s,e)=>{ended=true;Send(new { @event="stopped" });frame.Continue=false;};
                foreach(var pair in new[]{Tuple.Create("Started",(Delegate)onStart),Tuple.Create("Stopped",(Delegate)onStop)})
                    typeof(PlayController).GetEvent(pair.Item1,BindingFlags.Instance|BindingFlags.NonPublic).GetAddMethod(true).Invoke(action,new object[]{pair.Item2});
                // Parent owns cancellation. EOF terminates only this worker, never the game.
                new Thread(()=>{Console.ReadLine();Environment.Exit(0);}) { IsBackground=true }.Start();
                action.Play(new PlayActionArgs());
                if(!ended) Dispatcher.PushFrame(frame);
            } finally { foreach(var action in actions) action.Dispose(); }
        }
    }
}
// Match SDK serialization attributes used by plugin settings (Playnite, MIT).
class SdkResolver:DefaultContractResolver {
    protected override JsonProperty CreateProperty(MemberInfo member, MemberSerialization mode) {
        var property=base.CreateProperty(member,mode);
        property.Ignored |= Attribute.IsDefined(member,typeof(DontSerializeAttribute));
        var name=(SerializationPropertyNameAttribute)Attribute.GetCustomAttribute(member,typeof(SerializationPropertyNameAttribute));
        if(name!=null) property.PropertyName=name.PropertyName;
        return property;
    }
}
class ApiProxy:RealProxy {
    readonly Type type;
    internal ApiProxy(Type type):base(type) {this.type=type;}
    static object ReadJson(string method,object input,Type result) {
        if(method=="FromJson") return JsonConvert.DeserializeObject((string)input,result,AnikiPluginHost.Json);
        var stream=method=="FromJsonFile"?File.OpenRead((string)input):(Stream)input;
        using(var reader=new StreamReader(stream))
        using(var json=new JsonTextReader(reader))
            return JsonSerializer.Create(AnikiPluginHost.Json).Deserialize(json,result);
    }
    public override IMessage Invoke(IMessage raw) {
        var call=(IMethodCallMessage)raw;
        var method=(MethodInfo)call.MethodBase;

        try {
            object value;
            if(method.Name=="GetType") value=type;
            else if(type==typeof(IDataSerializer)) {
                if(method.Name=="TryFromJson" || method.Name=="TryFromJsonFile" || method.Name=="TryFromJsonStream") {
                    var args=(object[])call.Args.Clone();
                    Exception error=null;
                    try { args[1]=ReadJson(method.Name.Substring(3),args[0],method.GetGenericArguments()[0]); }
                    catch(Exception e) { args[1]=null; error=e; }
                    if(args.Length==3) args[2]=error;
                    return new ReturnMessage(error==null,args,args.Length,call.LogicalCallContext,call);
                }
                if(method.Name=="FromJsonFile" || method.Name=="FromJson" || method.Name=="FromJsonStream")
                    value=ReadJson(method.Name,call.Args[0],method.GetGenericArguments()[0]);
                else if(method.Name=="ToJson") value=JsonConvert.SerializeObject(call.Args[0],AnikiPluginHost.Json);
                else if(method.Name=="GetClone") value=JsonConvert.DeserializeObject(JsonConvert.SerializeObject(call.Args[0],AnikiPluginHost.Json),method.GetGenericArguments().Last(),AnikiPluginHost.Json);
                else throw new NotSupportedException("Unimplemented serializer: "+method.Name);
            }
            else if(type==typeof(IPlayniteAPI) && method.Name.StartsWith("get_") && method.ReturnType.IsInterface)
                value=new ApiProxy(method.ReturnType).GetTransparentProxy();
            else if(type==typeof(IPlaynitePathsAPI)) {
                switch(method.Name) {
                    case "get_ExtensionsDataPath":value=Path.Combine(AnikiPluginHost.Scratch,"data");break;
                    case "get_ConfigurationPath":value=AnikiPluginHost.Scratch;break;
                    case "get_ApplicationPath":value=AnikiPluginHost.Root;break;
                    case "get_IsPortable":value=true;break;
                    default:throw new NotSupportedException(method.Name);
                }
            } else if(type==typeof(IPlayniteInfoAPI)) {
                if(method.Name=="get_Mode") value=ApplicationMode.Fullscreen;
                else if(method.Name=="get_ApplicationVersion") value=Version.Parse(System.Diagnostics.FileVersionInfo.GetVersionInfo(Path.Combine(AnikiPluginHost.Root,"Playnite.DesktopApp.exe")).FileVersion);
                else if(method.ReturnType==typeof(bool)) value=false;
                else throw new NotSupportedException(method.Name);
            } else if(type==typeof(IPlayniteSettingsAPI) && method.Name=="get_Language") value="en_US";
            else if(type==typeof(IResourceProvider) && method.Name=="GetString") value=(string)call.Args[0];
            else if(type==typeof(IResourceProvider) && method.Name=="GetResource") {
                if((string)call.Args[0]=="FontIcoFont") value=new System.Windows.Media.FontFamily("Segoe UI");
                else throw new NotSupportedException("Unimplemented resource: "+call.Args[0]);
            }
            else throw new NotSupportedException("Unimplemented host API: "+type.Name+"."+method.Name);
            return new ReturnMessage(value,null,0,call.LogicalCallContext,call);
        } catch(Exception e) {return new ReturnMessage(e,call);}
    }
}
