module OR (
    IN,
    IN_2,
    OUT
);

input wire IN;
input wire IN_2;
output wire OUT;

wire net0;
wire net1;
wire net2;
wire net3;
wire net4;

assign net0 = IN;
assign net1 = IN_2;
assign OUT = net2;

assign net2 = ~(net3 & net4);
assign net3 = ~(net0 & net0);
assign net4 = ~(net1 & net1);

endmodule
