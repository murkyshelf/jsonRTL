`timescale 1ns / 1ps
module counter_tb;
    reg clk = 0;
    reg rst = 0;
    reg en = 0;
    wire [7:0] q;
    integer expected;
    counter dut (.clk(clk), .rst(rst), .en(en), .q(q));
    always #5 clk = ~clk;
    initial begin
        $dumpfile("counter.vcd");
        $dumpvars(0, counter_tb);
        #1 rst = 1;
        #2;
        if (q !== 0) $fatal(1, "reset failed");
        rst = 0;
        en = 1;
        for (expected = 1; expected <= 8; expected = expected + 1) begin
            @(posedge clk); #1;
            if (q !== expected) $fatal(1, "count failed: %d != %d", q, expected);
        end
        @(negedge clk); en = 0;
        repeat (2) begin
            @(posedge clk); #1;
            if (q !== 8) $fatal(1, "enable hold failed");
        end
        rst = 1; #1;
        if (q !== 0) $fatal(1, "asynchronous reset failed");
        $display("PASS: counter capture, enable, asynchronous reset");
        $finish;
    end
endmodule
