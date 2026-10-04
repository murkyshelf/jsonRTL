// The simulation and description sources are compiled unmodified. Only geometry,
// player input holders, keyboard polling, and audio sinks are substituted here.
namespace UnityEngine
{
    public enum FullScreenMode { ExclusiveFullScreen, FullScreenWindow, MaximizedWindow, Windowed }
    public static class Debug
    {
        public static void LogError(object error) => throw new System.Exception("Upstream DLS deserialization failed: " + error);
    }
    public struct Vector2
    {
        public float x, y;
        public Vector2(float x, float y) { this.x = x; this.y = y; }
        public static Vector2 zero => new(0, 0);
        public static Vector2 operator *(Vector2 v, float s) => new(v.x * s, v.y * s);
    }
    public struct Color
    {
        public float r, g, b, a;
        public Color(float r, float g, float b, float a = 1) { this.r = r; this.g = g; this.b = b; this.a = a; }
        public static Color clear => new(0, 0, 0, 0);
    }
}
namespace DLS.Graphics
{
    public static class DrawSettings
    {
        public const float GridSize = 0.125f;
        public const float ChipOutlineWidth = 0.025f;
        public const float SubChipPinInset = 0.0125f;
    }
}
namespace DLS.Game
{
    public static class GridHelper
    {
        public static float SnapToGrid(float v) => v;
        public static float SnapToGridForceEven(float v) => v;
    }
    public static class SubChipInstance
    {
        public static float MinChipHeightForPins(DLS.Description.PinDescription[] i, DLS.Description.PinDescription[] o)
            => System.Math.Max(i?.Length ?? 0, o?.Length ?? 0) * 0.25f;
    }
    public class DevPinInstance
    {
        public InputPinHolder Pin;
    }
    public class InputPinHolder
    {
        public DLS.Description.PinAddress Address;
        public uint PlayerInputState;
        public uint State;
    }
}
namespace DLS.Simulation
{
    public class SimAudio
    {
        public void InitFrame() { }
        public void NotifyAllNotesRegistered(double dt) { }
        public void RegisterNote(int f, uint v) { }
    }
    public static class SimKeyboardHelper
    {
        public static void RefreshInputState() { }
        public static bool KeyIsHeld(char c) => false;
    }
}
