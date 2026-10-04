// Export with:
// jsonrtl export examples/verilog/counter.v --top counter --out build/counter --clock clk
// In DLS, open counter_CLOCK, pulse rst high then low, and set en high.
module counter (
    input wire clk,
    input wire rst,
    input wire en,
    output reg [7:0] q
);
    always @(posedge clk or posedge rst) begin
        if (rst)
            q <= 8'b0;
        else if (en)
            q <= q + 8'b1;
    end
endmodule
