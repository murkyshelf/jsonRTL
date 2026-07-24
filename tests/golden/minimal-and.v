module Minimal_AND (
    a,
    b,
    y
);

input wire a;
input wire b;
output wire y;

wire a__2;
wire b__2;
wire y__2;

assign a__2 = a;
assign b__2 = b;
assign y = y__2;

assign y__2 = a__2 & b__2;

endmodule
