module XOR (
    IN,
    IN_2,
    OUT
);

input wire IN;
input wire IN_2;
output wire OUT;

wire net0;
wire net1;
wire net10;
wire net2;
wire net3;
wire net4;
wire net5;
wire net6;
wire net7;
wire net8;
wire net9;

assign net0 = IN;
assign net1 = IN_2;
assign OUT = net2;

assign net4 = ~(net0 & net3);
assign net5 = ~(net4 & net4);
assign net7 = ~(net6 & net1);
assign net8 = ~(net7 & net7);
assign net3 = ~(net1 & net1);
assign net6 = ~(net0 & net0);
assign net2 = ~(net9 & net10);
assign net9 = ~(net5 & net5);
assign net10 = ~(net8 & net8);

endmodule
