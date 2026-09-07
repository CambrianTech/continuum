using System;
using System.Drawing;
using System.Windows.Forms;

namespace MyFirstCSharpApp
{
    static class Program
    {
        [STAThread]
        static void Main()
        {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new Form1());
        }
    }

    public class Form1 : Form
    {
        private Label label1;

        public Form1()
        {
            InitializeComponent();
        }

        private void InitializeComponent()
        {
            this.label1 = new Label();
            this.label1.Text = "Hello, Sahar!";
            this.label1.Location = new Point(50, 50);
            this.Controls.Add(this.label1);

            this.ClientSize = new Size(320, 200);
            this.Text = "My First C# App";
            this.Load += Form1_Load;
        }

        private void Form1_Load(object sender, EventArgs e)
        {
            MessageBox.Show("Form loaded!");
        }
    }
}
