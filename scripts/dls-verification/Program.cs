using System.Text.Json;
using DLS.Description;
using DLS.Game;
using DLS.Simulation;

internal static class Program
{
    static readonly JsonSerializerOptions Json = new() { IncludeFields = true, PropertyNameCaseInsensitive = true };
    static int assertions;

    static int Main(string[] args)
    {
        try
        {
            string Option(string name, string fallback = null)
            {
                int i = Array.IndexOf(args, name);
                return i < 0 ? fallback : i + 1 < args.Length ? args[i + 1] : throw new Exception($"Missing {name} value");
            }
            string upstream = Option("--upstream", "/tmp/jsonrtl-dls-upstream");
            if (args.Contains("--self-test"))
            {
                SelfTest(upstream);
                Console.WriteLine(JsonSerializer.Serialize(new { result = "PASS", assertions, runtime = "Unmodified upstream DLS C# simulator" }));
                return 0;
            }
            string project = Option("--project") ?? throw new Exception("Specify --self-test or --project PATH --scenario PATH");
            string scenarioPath = Option("--scenario") ?? throw new Exception("Missing --scenario PATH");
            Scenario scenario = JsonSerializer.Deserialize<Scenario>(File.ReadAllText(scenarioPath), Json);
            if (scenario.Repetitions < 1 || scenario.SettleTicks < 1 || scenario.ClockSteps < 0 || scenario.Steps == null || scenario.Steps.Length == 0)
                throw new Exception("Scenario needs positive repetitions/settleTicks, nonnegative clockSteps, and at least one step");
            ChipLibrary library = LoadLibrary(project);
            ChipDescription desc = library.GetChipDescription(scenario.Chip);
            List<object> traces = new();
            for (int run = 0; run < scenario.Repetitions; run++)
            {
                Runner runner = new(desc, library, scenario.ClockSteps);
                List<object> samples = new();
                for (int i = 0; i < scenario.Steps.Length; i++)
                {
                    Step step = scenario.Steps[i];
                    runner.Advance(step.Inputs, step.Ticks ?? scenario.SettleTicks);
                    runner.Expect(step.Expect, $"{scenario.Chip} run {run} step {i}");
                    samples.Add(new { step = i, frame = Simulator.simulationFrame, outputs = runner.Outputs(), tristate = runner.Tristate() });
                }
                traces.Add(new { run, samples });
            }
            Console.WriteLine(JsonSerializer.Serialize(new { result = "PASS", assertions, chip = scenario.Chip, graph = CountGraph(desc, library), traces }));
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine(ex.ToString());
            return 1;
        }
    }

    public sealed class Scenario
    {
        public string Chip { get; set; }
        public int ClockSteps { get; set; } = 0;
        public int SettleTicks { get; set; } = 32;
        public int Repetitions { get; set; } = 1;
        public Step[] Steps { get; set; }
    }
    public sealed class Step
    {
        public Dictionary<string, uint> Inputs { get; set; } = new();
        public int? Ticks { get; set; }
        public Dictionary<string, uint> Expect { get; set; } = new();
    }

    static ChipLibrary LoadLibrary(string project)
    {
        string chips = Directory.Exists(Path.Combine(project, "Chips")) ? Path.Combine(project, "Chips") : project;
        string descriptionPath = Path.Combine(project, "ProjectDescription.json");
        string[] paths;
        if (File.Exists(descriptionPath))
        {
            ProjectDescription projectDescription = Serializer.DeserializeProjectDescription(File.ReadAllText(descriptionPath));
            if (projectDescription.AllCustomChipNames == null || projectDescription.StarredList == null || projectDescription.ChipCollections == null)
                throw new Exception("Project description has null chip listing, starred list, or collections");
            // These are accessed when opening a project in the upstream Loader.
            foreach (StarredItem item in projectDescription.StarredList) item.CacheDisplayStrings();
            foreach (ChipCollection collection in projectDescription.ChipCollections) collection.UpdateDisplayStrings();
            paths = projectDescription.AllCustomChipNames.Select(name => Path.Combine(chips, name + ".json")).ToArray();
        }
        else paths = Directory.GetFiles(chips, "*.json");
        ChipDescription[] custom = paths.Select(path => Serializer.DeserializeChipDescription(File.ReadAllText(path))).ToArray();
        HashSet<string> names = new(custom.Select(c => c.Name), ChipDescription.NameComparer);
        ChipDescription[] builtins = BuiltinChipCreator.CreateAllBuiltinChipDescriptions().Where(b => !names.Contains(b.Name)).ToArray();
        return new ChipLibrary(custom, builtins);
    }

    static Dictionary<string, int> CountGraph(ChipDescription desc, ChipLibrary library)
    {
        Dictionary<string, int> counts = new();
        void Visit(ChipDescription d)
        {
            string name = d.ChipType == ChipType.Custom ? "custom" : d.Name;
            counts[name] = counts.GetValueOrDefault(name) + 1;
            foreach (SubChipDescription s in d.SubChips) Visit(library.GetChipDescription(s.Name));
        }
        Visit(desc);
        return counts;
    }

    // Upstream AddConnection silently ignores invalid addresses. Fail before
    // construction so a malformed export cannot appear to pass through floating pins.
    static void ValidateGraph(ChipDescription root, ChipLibrary library)
    {
        HashSet<string> active = new(ChipDescription.NameComparer), checkedNames = new(ChipDescription.NameComparer);
        void Visit(ChipDescription d)
        {
            if (active.Contains(d.Name)) throw new Exception($"Recursive custom chip {d.Name}");
            if (!checkedNames.Add(d.Name)) return;
            if (d.InputPins == null || d.OutputPins == null || d.SubChips == null || d.Wires == null)
                throw new Exception($"Null description array in {d.Name}");
            active.Add(d.Name);
            HashSet<int> pinIDs = new();
            foreach (PinDescription p in d.InputPins.Concat(d.OutputPins))
                if (!pinIDs.Add(p.ID)) throw new Exception($"Duplicate pin ID in {d.Name}: {p.ID}");
            Dictionary<int, ChipDescription> children = new();
            foreach (SubChipDescription s in d.SubChips)
            {
                if (pinIDs.Contains(s.ID) || !children.TryAdd(s.ID, library.GetChipDescription(s.Name)))
                    throw new Exception($"Duplicate owner ID in {d.Name}: {s.ID}");
                Visit(children[s.ID]);
            }
            PinDescription Find(PinAddress a, bool source)
            {
                PinDescription[] pins;
                int id;
                if (children.TryGetValue(a.PinOwnerID, out ChipDescription child))
                {
                    pins = source ? child.OutputPins : child.InputPins;
                    id = a.PinID;
                }
                else
                {
                    pins = source ? d.InputPins : d.OutputPins;
                    id = a.PinOwnerID;
                    if (a.PinID != 0) throw new Exception($"Dev pin address must use PinID 0: {d.Name} {a}");
                }
                PinDescription[] matches = pins.Where(p => p.ID == id).ToArray();
                if (matches.Length != 1) throw new Exception($"Invalid {(source ? "source" : "target")} in {d.Name}: {a}");
                return matches[0];
            }
            foreach (WireDescription w in d.Wires)
            {
                PinDescription source = Find(w.SourcePinAddress, true), target = Find(w.TargetPinAddress, false);
                if (source.BitCount != target.BitCount) throw new Exception($"Wire width mismatch in {d.Name}: {source.Name} -> {target.Name}");
            }
            active.Remove(d.Name);
        }
        Visit(root);
    }

    sealed class Runner
    {
        readonly ChipDescription desc;
        readonly SimChip chip;
        readonly Dictionary<string, DevPinInstance> inputs;
        readonly DevPinInstance[] inputArray;
        readonly SimAudio audio = new();
        public Runner(ChipDescription desc, ChipLibrary library, int clockSteps = 0)
        {
            ValidateGraph(desc, library);
            this.desc = desc;
            chip = Simulator.BuildSimChip(desc, library);
            inputs = desc.InputPins.ToDictionary(p => p.Name, p => new DevPinInstance
            {
                Pin = new InputPinHolder { Address = new PinAddress(p.ID, 0), PlayerInputState = 0 }
            });
            inputArray = inputs.Values.ToArray();
            Simulator.Reset();
            Simulator.stepsPerClockTransition = clockSteps;
            // RunSimulationStep requests an order pass when the root changes.
        }
        public void Advance(Dictionary<string, uint> values, int ticks = 32)
        {
            foreach ((string name, uint value) in values)
            {
                PinDescription p = desc.InputPins.Single(p => p.Name == name);
                if (value > Mask(p)) throw new Exception($"Input {name} value {value} exceeds {(int)p.BitCount} bits");
                inputs[name].Pin.PlayerInputState = value;
            }
            if (ticks < 1) throw new Exception("ticks must be positive");
            for (int i = 0; i < ticks; i++) Simulator.RunSimulationStep(chip, inputArray, audio);
        }
        public Dictionary<string, uint> Outputs() => desc.OutputPins.ToDictionary(p => p.Name,
            p => chip.OutputPins.Single(s => s.ID == p.ID).State & Mask(p));
        public Dictionary<string, uint> Tristate() => desc.OutputPins.ToDictionary(p => p.Name,
            p => (chip.OutputPins.Single(s => s.ID == p.ID).State >> 16) & Mask(p));
        static uint Mask(PinDescription p) => (1u << (int)p.BitCount) - 1;
        public void Expect(Dictionary<string, uint> expected, string context)
        {
            Dictionary<string, uint> outputs = Outputs(), tri = Tristate();
            foreach ((string name, uint value) in expected)
            {
                assertions++;
                if (!outputs.TryGetValue(name, out uint actual)) throw new Exception($"{context}: no output {name}");
                if (actual != value || tri[name] != 0)
                    throw new Exception($"{context}: {name} expected {value}, actual {actual}, tristate {tri[name]}");
            }
        }
    }

    static Dictionary<string, uint> Values(params (string name, uint value)[] items) => items.ToDictionary(x => x.name, x => x.value);

    // Wrap native builtins because the public simulator entry point steps a custom root.
    static ChipDescription Wrap(ChipDescription builtin)
    {
        PinDescription[] inputs = builtin.InputPins.Select((p, i) => new PinDescription(p.Name, 100 + i, p.Position, p.BitCount, p.Colour, p.ValueDisplayMode)).ToArray();
        PinDescription[] outputs = builtin.OutputPins.Select((p, i) => new PinDescription(p.Name, 200 + i, p.Position, p.BitCount, p.Colour, p.ValueDisplayMode)).ToArray();
        List<WireDescription> wires = new();
        for (int i = 0; i < inputs.Length; i++) wires.Add(new() { SourcePinAddress = new(inputs[i].ID, 0), TargetPinAddress = new(1, builtin.InputPins[i].ID) });
        for (int i = 0; i < outputs.Length; i++) wires.Add(new() { SourcePinAddress = new(1, builtin.OutputPins[i].ID), TargetPinAddress = new(outputs[i].ID, 0) });
        return new ChipDescription { Name = "Harness " + builtin.Name, ChipType = ChipType.Custom, InputPins = inputs, OutputPins = outputs,
            SubChips = new[] { new SubChipDescription { Name = builtin.Name, ID = 1 } }, Wires = wires.ToArray() };
    }

    static void SelfTest(string upstream)
    {
        ChipLibrary library = LoadLibrary(Path.Combine(upstream, "TestData/Projects/MainTest"));
        Runner Builtin(string name, int clockSteps = 0) => new(Wrap(library.GetChipDescription(name)), library, clockSteps);
        Runner nand = Builtin("NAND");
        for (uint a = 0; a < 2; a++) for (uint b = 0; b < 2; b++)
        {
            nand.Advance(Values(("IN A", a), ("IN B", b)));
            nand.Expect(Values(("OUT", 1 ^ (a & b))), "native NAND");
        }
        foreach ((string name, int n) in new[] { ("4-1BIT", 4), ("8-1BIT", 8) })
        {
            Runner split = Builtin(name);
            for (uint value = 0; value < 1u << n; value++)
            {
                split.Advance(Values(("IN", value)), 1);
                split.Expect(Enumerable.Range(0, n).ToDictionary(i => "OUT " + (char)('A' + n - 1 - i), i => value >> (n - 1 - i) & 1), "native " + name);
            }
        }
        foreach ((string name, int n) in new[] { ("1-4BIT", 4), ("1-8BIT", 8) })
        {
            Runner merge = Builtin(name);
            for (uint value = 0; value < 1u << n; value++)
            {
                merge.Advance(Enumerable.Range(0, n).ToDictionary(i => "IN " + (char)('A' + n - 1 - i), i => value >> (n - 1 - i) & 1), 1);
                merge.Expect(Values(("OUT", value)), "native " + name);
            }
        }
        Runner splitNibbles = Builtin("8-4BIT"), mergeNibbles = Builtin("4-8BIT");
        for (uint value = 0; value < 256; value++)
        {
            // The real simulator refreshes its order pass when switching roots.
            splitNibbles.Advance(Values(("IN", value)), 1);
            splitNibbles.Expect(Values(("OUT B", value >> 4), ("OUT A", value & 15)), "native 8-4BIT");
            mergeNibbles.Advance(Values(("IN B", value >> 4), ("IN A", value & 15)), 1);
            mergeNibbles.Expect(Values(("OUT", value)), "native 4-8BIT");
        }
        Runner clock = Builtin("CLOCK", 2);
        for (int frame = 1; frame <= 8; frame++)
        {
            clock.Advance(Values(), 1);
            clock.Expect(Values(("CLK", ((frame / 2) & 1) == 0 ? 1u : 0u)), "native CLOCK");
        }
        Runner not = new(library.GetChipDescription("NOT"), library);
        for (uint value = 0; value < 2; value++)
        {
            not.Advance(Values(("IN", value)));
            not.Expect(Values(("OUT", 1 ^ value)), "native custom NOT");
        }
        for (int repetition = 0; repetition < 20; repetition++)
        {
            Runner latch = new(library.GetChipDescription("D-LATCH"), library);
            latch.Advance(Values(("DATA", 0), ("STORE", 1)));
            latch.Expect(Values(("OUT", 0)), "native latch capture 0");
            latch.Advance(Values(("DATA", 1)));
            latch.Expect(Values(("OUT", 1)), "native latch transparent");
            latch.Advance(Values(("STORE", 0)));
            latch.Advance(Values(("DATA", 0)));
            latch.Expect(Values(("OUT", 1)), "native latch hold");
            Runner ff = new(library.GetChipDescription("FLIP-FLOP"), library);
            ff.Advance(Values(("DATA", 0), ("CLOCK", 0)));
            ff.Advance(Values(("CLOCK", 1)));
            ff.Expect(Values(("OUT", 0)), "native FF rising capture 0");
            ff.Advance(Values(("DATA", 1)));
            ff.Expect(Values(("OUT", 0)), "native FF high hold");
            ff.Advance(Values(("CLOCK", 0)));
            ff.Expect(Values(("OUT", 0)), "native FF falling hold");
            ff.Advance(Values(("CLOCK", 1)));
            ff.Expect(Values(("OUT", 1)), "native FF rising capture 1");
        }
    }
}
