module Eight_bit_Nibble_Swap (
    data_in,
    data_out
);

input wire [7:0] data_in;
output wire [7:0] data_out;

wire [7:0] data;
wire [7:0] result;

assign data = data_in;
assign data_out = result;

assign result[3:0] = data[7:4];
assign result[7:4] = data[3:0];

endmodule
