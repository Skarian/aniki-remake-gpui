// Integration fixture: owns its own game-like window and deliberately leaves
// its process alive after the window closes, like an emulator finishing exit.
using System;
using System.IO;
using System.Windows.Forms;
using System.Runtime.InteropServices;
class WindowFixture {
    // Some games create a visible window without activating it. The launcher
    // must perform the handoff rather than relying on the child to do it.
    class PassiveForm : Form {
        protected override bool ShowWithoutActivation { get { return true; } }
    }
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr hwnd);
    [STAThread] static void Main(string[] args) {
        string mode = args[0], output = args[1];
        if(mode == "immediate") return;
        Form form = mode == "noactivate" ? new PassiveForm() : new Form();
        form.Text = "Aniki lifecycle fixture"; form.Width = 900; form.Height = 600;
        if(mode == "fullscreen") { form.FormBorderStyle = FormBorderStyle.None; form.WindowState = FormWindowState.Maximized; }
        if(mode == "delayed" || mode == "late") {
            form.Text = "Game splash";
            var ready = new Timer { Interval = mode == "late" ? 95000 : 5000 };
            ready.Tick += (s,e) => { ready.Stop(); form.Text = "Aniki lifecycle fixture"; File.WriteAllText(output + ".ready", "ready"); };
            ready.Start();
        }
        var context = new ApplicationContext();
        form.Shown += (s,e) => {
            File.WriteAllText(output, form.Handle.ToInt64().ToString());
            if(mode != "noactivate") { form.Activate(); SetForegroundWindow(form.Handle); }
        };
        var timer = new Timer { Interval = mode == "late" ? 102000 : mode == "delayed" ? 11000 : 5000 };
        timer.Tick += (s,e) => {
            timer.Stop();
            if(mode == "game-crash") Environment.Exit(7);
            if(mode == "exit" || mode == "delayed" || mode == "late" || mode == "fullscreen" || mode == "noactivate") { form.Close(); Application.Exit(); }
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
