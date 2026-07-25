module n_1_bit_adder (
    A,
    B,
    cin,
    carry,
    out
);

input wire A;
input wire B;
input wire cin;
output wire carry;
output wire out;

wire net0;
wire net1;
wire net10;
wire net11;
wire net12;
wire net13;
wire net14;
wire net15;
wire net16;
wire net17;
wire net18;
wire net19;
wire net2;
wire net20;
wire net21;
wire net22;
wire net23;
wire net24;
wire net25;
wire net26;
wire net27;
wire net3;
wire net4;
wire net5;
wire net6;
wire net7;
wire net8;
wire net9;

assign net0 = A;
assign net1 = B;
assign net2 = cin;
assign carry = net3;
assign out = net4;

assign net6 = ~(net0 & net5);
assign net7 = ~(net6 & net6);
assign net16 = ~(net15 & net15);
assign net18 = ~(net17 & net13);
assign net19 = ~(net18 & net18);
assign net14 = ~(net13 & net13);
assign net17 = ~(net2 & net2);
assign net4 = ~(net20 & net21);
assign net20 = ~(net16 & net16);
assign net21 = ~(net19 & net19);
assign net22 = ~(net0 & net1);
assign net23 = ~(net22 & net22);
assign net9 = ~(net8 & net1);
assign net24 = ~(net2 & net13);
assign net25 = ~(net24 & net24);
assign net3 = ~(net26 & net27);
assign net26 = ~(net23 & net23);
assign net27 = ~(net25 & net25);
assign net10 = ~(net9 & net9);
assign net5 = ~(net1 & net1);
assign net8 = ~(net0 & net0);
assign net13 = ~(net11 & net12);
assign net11 = ~(net7 & net7);
assign net12 = ~(net10 & net10);
assign net15 = ~(net2 & net14);

endmodule
