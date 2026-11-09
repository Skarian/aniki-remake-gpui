// Integration fixture: owns its own game-like window and deliberately leaves
// its process alive after the window closes, like an emulator finishing exit.
using System;
using System.IO;
using System.Windows.Forms;
using System.Runtime.InteropServices;
class WindowFixture {
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr hwnd);
    [STAThread] static void Main(string[] args) {
        string mode = args[0], output = args[1];
        var form = new Form { Text = "Aniki lifecycle fixture", Width = 900, Height = 600 };
        var context = new ApplicationContext();
        form.Shown += (s,e) => {
            File.WriteAllText(output, form.Handle.ToInt64().ToString());
            form.Activate(); SetForegroundWindow(form.Handle);
        };
        var timer = new Timer { Interval = 5000 };
        timer.Tick += (s,e) => {
            timer.Stop();
            if(mode == "game-crash") Environment.Exit(7);
            if(mode == "exit") { form.Close(); Application.Exit(); }
            else if(mode == "minimize") form.WindowState = FormWindowState.Minimized;
            else if(mode == "replace") {
                form.Hide();
                var replacement = new Timer { Interval = 700 };
                replacement.Tick += (a,b) => { replacement.Stop(); form.Show(); form.Activate(); };
                replacement.Start();
            } else form.Close();
            File.WriteAllText(output + ".transition", mode);
        };
        form.Show(); timer.Start();
        Application.Run(context);
    }
}
