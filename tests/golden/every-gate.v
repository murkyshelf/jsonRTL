module every_gate (
    a,
    b,
    and_y,
    buffer_y,
    const_y,
    nand_y,
    nor_y,
    not_y,
    or_y,
    xnor_y,
    xor_y
);

input wire a;
input wire b;
output wire and_y;
output wire buffer_y;
output wire const_y;
output wire nand_y;
output wire nor_y;
output wire not_y;
output wire or_y;
output wire xnor_y;
output wire xor_y;

wire a_net;
wire and_net;
wire b_net;
wire buffer_net;
wire const_net;
wire nand_net;
wire nor_net;
wire not_net;
wire or_net;
wire xnor_net;
wire xor_net;

assign a_net = a;
assign b_net = b;
assign and_y = and_net;
assign buffer_y = buffer_net;
assign const_y = const_net;
assign nand_y = nand_net;
assign nor_y = nor_net;
assign not_y = not_net;
assign or_y = or_net;
assign xnor_y = xnor_net;
assign xor_y = xor_net;

assign and_net = a_net & b_net;
assign buffer_net = a_net;
assign const_net = 1'b1;
assign nand_net = ~(a_net & b_net);
assign nor_net = ~(a_net | b_net);
assign not_net = ~a_net;
assign or_net = a_net | b_net;
assign xnor_net = ~(a_net ^ b_net);
assign xor_net = a_net ^ b_net;

endmodule
