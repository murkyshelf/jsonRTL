module NOT (
    IN,
    OUT
);

input wire IN;
output wire OUT;

wire net0;
wire net1;

assign net0 = IN;
assign OUT = net1;

assign net1 = ~(net0 & net0);

endmodule
